use lira::{Arch, Operation, Snippet, StatementSeq};

#[test]
fn round_trip() -> anyhow::Result<()> {
    let generic = "\
1 8 a = get a;
1 8 b = get b;
1 1 c = op.g sgt a b;
1 8 d = op.g select c a b;
1 8 e = op add_8 a;
1 8 f = op mystery_8 d b;
";
    let specialized = "\
1 8 a = get a;
1 8 b = get b;
1 1 c = op sgt_8 a b;
1 8 d = op select_8 c a b;
1 8 e = op add_8 a;
1 8 f = op mystery_8 d b;
";

    let mut arch = Arch::default();
    arch.operations = vec![Operation {
        name: "add_8".to_string(),
        attributes: vec![],
        inputs: vec![8, 8, 8],
        outputs: vec![8],
        semantic_base: Some("add".to_string()),
        semantic_func: None,
        semantic_table: None,
    }];
    arch.snippets.push(Snippet {
        name: "s".to_string(),
        attributes: vec![],
        seq: StatementSeq::parse(generic).unwrap(),
    });

    let mut arch = lira_opt::Arch(arch);
    arch.specialize_operations()?;
    assert_eq!(
        arch.0.snippets[0].seq,
        StatementSeq::parse(specialized).unwrap()
    );
    assert_eq!(arch.0.operations.len(), 3);
    assert_eq!(arch.0.operations[1].name, "sgt_8");
    assert_eq!(arch.0.operations[1].semantic_base.as_deref(), Some("sgt"));
    assert_eq!(arch.0.operations[2].name, "select_8");

    // `add_8` has a base but a mismatched arrow, `mystery_8` has no base: both
    // survive generalization unchanged.
    arch.generalize_operations()?;
    assert_eq!(
        arch.0.snippets[0].seq,
        StatementSeq::parse(generic).unwrap()
    );

    arch.specialize_operations()?;
    assert_eq!(
        arch.0.snippets[0].seq,
        StatementSeq::parse(specialized).unwrap()
    );

    Ok(())
}
