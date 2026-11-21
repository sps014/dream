use bumpalo::Bump;
use bumpalo_herd::Member;

/// Where a parser allocates its nodes: one arena, or this thread's member of a shared herd so
/// several files can be parsed at once into nodes with the same lifetime.
#[derive(Clone, Copy)]
pub enum AstArena<'a, 'm> {
    Bump(&'a Bump),
    Member(&'m Member<'a>),
}

impl<'a> From<&'a Bump> for AstArena<'a, '_> {
    fn from(bump: &'a Bump) -> Self {
        AstArena::Bump(bump)
    }
}

impl<'a, 'm> From<&'m Member<'a>> for AstArena<'a, 'm> {
    fn from(member: &'m Member<'a>) -> Self {
        AstArena::Member(member)
    }
}

impl<'a> AstArena<'a, '_> {
    pub(crate) fn alloc<T>(self, value: T) -> &'a mut T {
        match self {
            AstArena::Bump(bump) => bump.alloc(value),
            AstArena::Member(member) => member.alloc(value),
        }
    }

    pub(crate) fn alloc_slice_fill_iter<T, I>(self, iter: I) -> &'a mut [T]
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        match self {
            AstArena::Bump(bump) => bump.alloc_slice_fill_iter(iter),
            AstArena::Member(member) => member.alloc_slice_fill_iter(iter),
        }
    }

    pub(crate) fn alloc_slice_clone<T: Clone>(self, src: &[T]) -> &'a mut [T] {
        match self {
            AstArena::Bump(bump) => bump.alloc_slice_clone(src),
            AstArena::Member(member) => member.alloc_slice_clone(src),
        }
    }
}
