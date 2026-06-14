use nebula_ast::Type;

pub(crate) fn types_equal(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Int, Type::Int)
        | (Type::Float, Type::Float)
        | (Type::Bool, Type::Bool)
        | (Type::Str, Type::Str)
        | (Type::Void, Type::Void) => true,
        (Type::List(a), Type::List(b)) => types_equal(a, b),
        (Type::Map(ak, av), Type::Map(bk, bv)) => types_equal(ak, bk) && types_equal(av, bv),
        (Type::Option(a), Type::Option(b)) => types_equal(a, b),
        (Type::NoneValue, Type::NoneValue) => true,
        (Type::NoneValue, Type::Option(_)) | (Type::Option(_), Type::NoneValue) => true,
        (Type::Named(a), Type::Named(b)) => a == b,
        (Type::Fn(ap, ar), Type::Fn(bp, br)) => {
            ap.len() == bp.len()
                && ap.iter().zip(bp.iter()).all(|(x, y)| types_equal(x, y))
                && types_equal(ar, br)
        }
        _ => false,
    }
}