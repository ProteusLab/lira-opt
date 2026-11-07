use std::sync::atomic::{AtomicBool, Ordering};

use ahash::AHashSet;
use anyhow::{Context, Result};
use lira::{Arch, ArchIndex, Shape, Statement, StatementSeq};
use rayon::prelude::*;

use crate::ir_ops::BASE_OP_SIGNATURE;
use crate::ir_utils::iter_stmt_seq_with_inputs;

/// Validates every instruction and snippet in parallel, reporting each failing
/// sequence with `log::error!`.
pub fn validate_arch(
    arch: &Arch,
    custom_kinds: &[&str],
    mult_divides: &[(&str, &str)],
) -> Result<()> {
    let index = Arch::build_index(arch);
    let failed = AtomicBool::new(false);
    crate::arch_utils::iter_arch_stmt_seq(arch)
        .par_bridge()
        .for_each(|(name, stmts)| {
            if let Err(error) = validate_stmts_signatures(&index, stmts, custom_kinds, mult_divides)
            {
                failed.store(true, Ordering::Relaxed);
                log::error!("{name}: {error:#}");
            }
        });
    anyhow::ensure!(
        !failed.load(Ordering::Relaxed),
        "some checks failed (details - env_logger)"
    );
    Ok(())
}

/// Best-effort syntactical-level check. In most cases should be sufficient.
pub fn validate_stmts_signatures(
    index: &ArchIndex,
    stmts: &StatementSeq,
    custom_kinds: &[&str],
    mult_divides: &[(&str, &str)],
) -> Result<()> {
    validate_stmts_def_use(stmts)?;
    for (stmt, inputs) in iter_stmt_seq_with_inputs(stmts) {
        let err = || stmt.to_string();
        validate_stmt(index, stmt, &inputs, custom_kinds, mult_divides).with_context(err)?;
    }
    Ok(())
}

/// Basic sanity check. It's okay in most cases to assume it holds (panic if not).
pub fn validate_stmts_def_use(stmts: &StatementSeq) -> Result<()> {
    let mut defined = AHashSet::new();
    for stmt in stmts.iter() {
        for input in stmt.inputs.iter() {
            anyhow::ensure!(defined.contains(input.as_str()));
        }
        for output in stmt.outputs.iter() {
            anyhow::ensure!(defined.insert(output.as_str()));
        }
    }
    Ok(())
}

fn validate_stmt(
    index: &ArchIndex,
    stmt: &Statement,
    inputs: &[(Shape, usize)],
    custom_kinds: &[&str],
    mult_divides: &[(&str, &str)],
) -> Result<()> {
    if custom_kinds.contains(&stmt.kind.as_str()) {
        return Ok(());
    }

    let spec = stmt.specifier.as_str();
    let shape = &stmt.shape;
    let widths = || inputs.iter().map(|(_, width)| *width).collect::<Vec<_>>();

    let input_shapes_eq_out = || inputs.iter().all(|(sh, _)| sh == shape);
    let widths_match = |expected: &[usize], got: &[(Shape, usize)]| {
        expected.iter().eq(got.iter().map(|(_, width)| width))
    };

    macro_rules! get {
        ($field:ident) => {{
            let err = || anyhow::anyhow!("no {} {spec}", stringify!($field));
            index.$field.get(spec).with_context(err)
        }};
    }
    let op_signature = || {
        let err = || anyhow::anyhow!("no op base {spec}");
        BASE_OP_SIGNATURE.get(spec).with_context(err)
    };

    let divides = |small: &str, large: &str| mult_divides.contains(&(small, large));

    // scan/fold read values as two-dimensional: the first input's shape is the
    // inner (state) shape, the last input's is the full 2-D (vector) shape.
    let check_scan_fold = |state_dim: usize| {
        anyhow::ensure!(state_dim >= 1 && state_dim < inputs.len());
        let inner = &inputs[0].0;
        let two_d = &inputs[inputs.len() - 1].0;
        anyhow::ensure!(inputs[..state_dim].iter().all(|(sh, _)| sh == inner));
        anyhow::ensure!(inputs[state_dim..].iter().all(|(sh, _)| sh == two_d));
        anyhow::ensure!(crate::shape_divides(inner, two_d, &divides));
        Ok(())
    };

    let check_vector_reshape = || {
        anyhow::ensure!(inputs.len() == 1);
        anyhow::ensure!(stmt.outputs.len() == 1);
        anyhow::ensure!(inputs[0].1 == stmt.outputs_types[0]);
        Ok(())
    };

    let check_rf = |rsi: &Shape, width: usize, val_shape: &Shape| {
        anyhow::ensure!(rsi.is_scalar());
        let rf = get!(rf)?;
        anyhow::ensure!(crate::shape_mul(shape, width) == rf.reg_size);
        anyhow::ensure!(val_shape == shape);
        Ok(())
    };

    // Everything generic for `cond.*`: returns the inputs a kind still needs.
    let split_cond_stmt = || {
        anyhow::ensure!(inputs.len() >= 1 + stmt.outputs_types.len());
        let (condition, rest) = inputs.split_at(1);
        anyhow::ensure!(condition[0].1 == 1);
        anyhow::ensure!(condition[0].0 == *shape);
        let (on_false, rest) = rest.split_at(stmt.outputs_types.len());
        anyhow::ensure!(on_false.iter().all(|(sh, _)| sh == shape));
        anyhow::ensure!(widths_match(&stmt.outputs_types, on_false));
        Ok::<_, anyhow::Error>(rest)
    };

    let check_sys_reg = |is_write: bool, inputs: &[(Shape, usize)]| {
        anyhow::ensure!(stmt.outputs.len() == if is_write { 0 } else { 1 });
        anyhow::ensure!(inputs.len() == if is_write { 1 } else { 0 });
        if is_write {
            anyhow::ensure!(inputs[0].0 == stmt.shape);
        }
        get!(sr)?;
        Ok(())
    };

    match stmt.kind.as_str() {
        "index" | "const" | "dyn_const" | "input" => {
            anyhow::ensure!(inputs.is_empty());
            anyhow::ensure!(stmt.outputs.len() == 1);
            match stmt.kind.as_str() {
                "const" => anyhow::ensure!(crate::stmt_const_parse(spec).is_ok()),
                "input" => anyhow::ensure!(spec.parse::<usize>().is_ok()),
                "index" => anyhow::ensure!(spec.parse::<usize>().is_ok()),
                _ => {}
            }
        }
        "output" => {
            anyhow::ensure!(stmt.outputs.is_empty());
            anyhow::ensure!(inputs.len() == 1);
            anyhow::ensure!(stmt.specifier.parse::<usize>().is_ok());
        }

        "op" => {
            anyhow::ensure!(input_shapes_eq_out());
            let op = get!(op)?;
            anyhow::ensure!(widths_match(&op.inputs, inputs));
            anyhow::ensure!(stmt.outputs_types == op.outputs);
        }
        "op.g" => {
            anyhow::ensure!(input_shapes_eq_out());
            let parse = op_signature()?.1;
            parse(&stmt.outputs_types, &widths())?;
        }
        "fold.op" => {
            let state_dim = stmt.outputs.len();
            check_scan_fold(state_dim)?;
            anyhow::ensure!(stmt.shape == inputs[0].0);
            let op = get!(op)?;
            anyhow::ensure!(widths_match(&op.inputs, inputs));
            anyhow::ensure!(op.outputs.len() >= state_dim);
            anyhow::ensure!(stmt.outputs_types == op.outputs[..state_dim]);
        }
        "fold.op.g" => {
            let state_dim = stmt.outputs.len();
            check_scan_fold(state_dim)?;
            anyhow::ensure!(stmt.shape == inputs[0].0);
            let ((outputs, inputs_count), _) = *op_signature()?;
            anyhow::ensure!(outputs >= state_dim);
            anyhow::ensure!(inputs.len() == inputs_count);
        }
        "scan.op" => {
            let op = get!(op)?;
            let state_dim = op
                .outputs
                .len()
                .checked_sub(stmt.outputs.len())
                .context("more outputs than the operation has")?;
            check_scan_fold(state_dim)?;
            anyhow::ensure!(stmt.shape == inputs[inputs.len() - 1].0);
            anyhow::ensure!(widths_match(&op.inputs, inputs));
            anyhow::ensure!(stmt.outputs_types.as_slice() == &op.outputs[state_dim..]);
        }
        "scan.op.g" => {
            let ((outputs, inputs_count), _) = *op_signature()?;
            let state_dim = outputs
                .checked_sub(stmt.outputs.len())
                .context("more outputs than the signature has")?;
            check_scan_fold(state_dim)?;
            anyhow::ensure!(stmt.shape == inputs[inputs.len() - 1].0);
            anyhow::ensure!(inputs.len() == inputs_count);
        }

        "env" | "cond.env" => {
            let env = get!(env)?;
            anyhow::ensure!(stmt.outputs_types == env.outputs);
            let env_inputs = match stmt.kind == "env" {
                true => inputs,
                false => split_cond_stmt()?,
            };
            anyhow::ensure!(env_inputs.iter().all(|(sh, _)| sh == shape));
            anyhow::ensure!(widths_match(&env.inputs, env_inputs), "env inputs");
        }

        "cond.pc.read" => {
            anyhow::ensure!(shape.is_scalar());
            anyhow::ensure!(stmt.outputs.len() == 1);
            anyhow::ensure!(split_cond_stmt()?.is_empty());
        }
        "cond.pc.write" => {
            anyhow::ensure!(shape.is_scalar());
            anyhow::ensure!(stmt.outputs.is_empty());
            let rest = split_cond_stmt()?;
            anyhow::ensure!(rest.len() == 1);
            anyhow::ensure!(rest[0].0 == stmt.shape);
        }

        "broadcast" => {
            check_vector_reshape()?;
            anyhow::ensure!(inputs[0].0.is_scalar());
        }
        "extract_first" => {
            check_vector_reshape()?;
            anyhow::ensure!(crate::shape_le(shape, &inputs[0].0));
        }
        "extend_zero" | "extend_ones" => {
            check_vector_reshape()?;
            anyhow::ensure!(crate::shape_le(&inputs[0].0, shape));
        }

        "gather" => {
            anyhow::ensure!(inputs.len() == 3);
            anyhow::ensure!(stmt.outputs.len() == 1);
            anyhow::ensure!(inputs[1].0 == stmt.shape);
            anyhow::ensure!(inputs[2].0 == stmt.shape);
            anyhow::ensure!(inputs[0].1 == stmt.outputs_types[0]);
        }
        "cvi" => {
            anyhow::ensure!(inputs.len() == 4);
            anyhow::ensure!(stmt.outputs.len() == 1);
            anyhow::ensure!(inputs[0].0 == stmt.shape);
            anyhow::ensure!(inputs[1].0 == inputs[2].0 && inputs[2].0 == inputs[3].0);
            anyhow::ensure!(inputs[0].1 == stmt.outputs_types[0]);
        }

        "read" => {
            anyhow::ensure!(stmt.outputs.len() == 1);
            anyhow::ensure!(stmt.inputs.len() == 1);
            check_rf(&inputs[0].0, stmt.outputs_types[0], shape)?;
        }
        "write" => {
            anyhow::ensure!(stmt.outputs.len() == 0);
            anyhow::ensure!(stmt.inputs.len() == 2);
            check_rf(&inputs[0].0, inputs[1].1, &inputs[1].0)?;
        }
        "cond.write" => {
            anyhow::ensure!(stmt.outputs.len() == 0);
            anyhow::ensure!(stmt.inputs.len() == 3);
            let inputs = split_cond_stmt()?;
            check_rf(&inputs[0].0, inputs[1].1, &inputs[1].0)?;
        }

        "sys_reg.read" => check_sys_reg(false, inputs)?,
        "cond.sys_reg.read" => check_sys_reg(false, split_cond_stmt()?)?,
        "sys_reg.write" => check_sys_reg(true, inputs)?,
        "cond.sys_reg.write" => check_sys_reg(true, split_cond_stmt()?)?,

        "snippet" => {
            // TODO: implement. Analyze snippet's statements - inputs and outputs.
            //  Make sure their shapes/types agree with how we call snippet.
            //  Note: not all passed values have to be used, same for outputs.
        }

        kind => anyhow::bail!("unknown kind {kind}"),
    }

    Ok(())
}
