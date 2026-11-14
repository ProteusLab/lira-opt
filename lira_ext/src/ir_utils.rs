use ahash::AHashMap;
use lira::{Shape, Statement, StatementSeq};

/// Yields every statement together with the shape and width of each input.
///
/// *Panics* if `stmts` are ill-formed: used input was not defined
pub fn iter_stmt_seq_with_inputs(
    stmts: &StatementSeq,
) -> impl Iterator<Item = (&Statement, Vec<(Shape, usize)>)> {
    let mut values: AHashMap<&str, (Shape, usize)> = AHashMap::new();
    stmts.iter().map(move |stmt| {
        let map = |name: &String| values[name.as_str()].clone();
        let inputs = stmt.inputs.iter().map(map).collect();
        for (name, ty) in stmt.outputs.iter().zip(stmt.outputs_types.iter()) {
            values.insert(name.as_str(), (stmt.shape.clone(), *ty));
        }
        (stmt, inputs)
    })
}

pub fn shape_divides(
    small: &Shape,
    large: &Shape,
    mult_divides: impl Fn(&str, &str) -> bool,
) -> bool {
    if large.lanes_base % small.lanes_base != 0 {
        return false;
    }
    match (&small.lanes_mult, &large.lanes_mult) {
        (None, None) => true,
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (Some(a), Some(b)) => a == b || mult_divides(a, b),
    }
}

/// Whether the lane count of `small` is always less than the lane count of
/// `large`, for every value of the dynamic constants.
/// Scales the lane count by `factor`, keeping the dynamic multiplier.
pub fn shape_mul(shape: &Shape, factor: usize) -> Shape {
    Shape::new(shape.lanes_base * factor, shape.lanes_mult.clone())
}

pub fn shape_lt(small: &Shape, large: &Shape) -> bool {
    match (&small.lanes_mult, &large.lanes_mult) {
        (None, None) => small.lanes_base < large.lanes_base,
        (Some(a), Some(b)) if a == b => small.lanes_base < large.lanes_base,
        (None, Some(_)) => small.lanes_base < large.lanes_base,
        _ => false,
    }
}

/// Whether the lane count of `small` is always at most the lane count of
/// `large`, for every value of the dynamic constants.
pub fn shape_le(small: &Shape, large: &Shape) -> bool {
    match (&small.lanes_mult, &large.lanes_mult) {
        (None, None) => small.lanes_base <= large.lanes_base,
        (Some(a), Some(b)) if a == b => small.lanes_base <= large.lanes_base,
        (None, Some(_)) => small.lanes_base <= large.lanes_base,
        _ => false,
    }
}
