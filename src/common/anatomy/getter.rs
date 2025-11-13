use super::Organ;


pub trait FieldGet
{
    type T<'a, O>
    where
        Self: 'a,
        O: 'a;
}

pub trait PartFieldGetter<F: FieldGet>
{
    type V<'a>
    where
        F: 'a;

    fn get<'a, O: Organ + 'a>(value: F::T<'a, O>) -> Self::V<'a>;
}
