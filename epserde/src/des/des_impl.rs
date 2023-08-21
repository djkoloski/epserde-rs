/*
 * SPDX-FileCopyrightText: 2023 Inria
 * SPDX-FileCopyrightText: 2023 Sebastiano Vigna
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

use core::mem::MaybeUninit;

use crate::des::*;
use crate::Align;
use crate::CopySelector;
use crate::CopyType;
use crate::Eps;
use crate::EpsCopy;
use crate::Zero;
use crate::ZeroCopy;

macro_rules! impl_prim{
    ($($ty:ty),*) => {$(
        impl DeserializeInner for $ty {
            #[inline(always)]
            fn _deserialize_full_copy_inner(mut backend:Cursor) -> Result<(Self,Cursor), DeserializeError> {
                backend = <$ty>::pad_align_and_check(backend)?;
                Ok((
                    <$ty>::from_ne_bytes(backend.data[..core::mem::size_of::<$ty>()].try_into().unwrap()),
                    backend.skip(core::mem::size_of::<$ty>()),
                ))
            }
            type DeserType<'a> = $ty;
            #[inline(always)]
            fn _deserialize_eps_copy_inner(
                backend: Cursor,
            ) -> Result<(Self::DeserType<'_>, Cursor), DeserializeError> {
                <$ty as DeserializeInner>::_deserialize_full_copy_inner(backend)
            }
        }
    )*};
}

impl_prim!(isize, i8, i16, i32, i64, i128, usize, u8, u16, u32, u64, u128, f32, f64);

impl DeserializeInner for () {
    #[inline(always)]
    fn _deserialize_full_copy_inner(backend: Cursor) -> Result<(Self, Cursor), DeserializeError> {
        Ok(((), backend))
    }
    type DeserType<'a> = Self;
    #[inline(always)]
    fn _deserialize_eps_copy_inner(backend: Cursor) -> Result<(Self, Cursor), DeserializeError> {
        Self::_deserialize_full_copy_inner(backend)
    }
}

impl DeserializeInner for bool {
    #[inline(always)]
    fn _deserialize_full_copy_inner(backend: Cursor) -> Result<(Self, Cursor), DeserializeError> {
        Ok((backend.data[0] != 0, backend.skip(1)))
    }
    type DeserType<'a> = Self;
    #[inline(always)]
    fn _deserialize_eps_copy_inner(backend: Cursor) -> Result<(Self, Cursor), DeserializeError> {
        Self::_deserialize_full_copy_inner(backend)
    }
}

impl DeserializeInner for char {
    #[inline(always)]
    fn _deserialize_full_copy_inner(backend: Cursor) -> Result<(Self, Cursor), DeserializeError> {
        u32::_deserialize_full_copy_inner(backend).map(|(x, y)| (char::from_u32(x).unwrap(), y))
    }
    type DeserType<'a> = Self;
    #[inline(always)]
    fn _deserialize_eps_copy_inner(backend: Cursor) -> Result<(Self, Cursor), DeserializeError> {
        Self::_deserialize_full_copy_inner(backend)
    }
}

////////////////////////////////////////////////////////////////////////////////

fn deserialize_slice<T: ZeroCopy>(backend: Cursor) -> Result<(&'_ [T], Cursor), DeserializeError> {
    let (len, mut backend) = usize::_deserialize_full_copy_inner(backend)?;
    let bytes = len * core::mem::size_of::<T>();
    // a slice can only be deserialized with zero copy
    // outerwise you need a vec, TODO!: how do we enforce this at compile time?
    backend = T::pad_align_and_check(backend)?;
    let (pre, data, after) = unsafe { backend.data[..bytes].align_to::<T>() };
    debug_assert!(pre.is_empty());
    debug_assert!(after.is_empty());
    Ok((data, backend.skip(bytes)))
}

fn deserialize_array_zero<T: DeserializeInner, const N: usize>(
    mut backend: Cursor,
) -> Result<(&'_ [T; N], Cursor), DeserializeError> {
    let bytes = std::mem::size_of::<[T; N]>();
    backend = T::pad_align_and_check(backend)?;
    let (pre, data, after) = unsafe { backend.data[..bytes].align_to::<[T; N]>() };
    debug_assert!(pre.is_empty());
    debug_assert!(after.is_empty());
    Ok((&data[0], backend.skip(bytes)))
}

fn deserialize_array_eps<T: DeserializeInner, const N: usize>(
    mut backend: Cursor,
) -> Result<([<T as DeserializeInner>::DeserType<'_>; N], Cursor), DeserializeError> {
    backend = T::pad_align_and_check(backend)?;
    let mut res = MaybeUninit::<[<T as DeserializeInner>::DeserType<'_>; N]>::uninit();
    unsafe {
        for item in &mut res.assume_init_mut().iter_mut() {
            let (elem, new_backend) = T::_deserialize_eps_copy_inner(backend)?;
            std::ptr::write(item, elem);
            backend = new_backend;
        }
        Ok((res.assume_init(), backend))
    }
}

fn deserialize_array_full<T: DeserializeInner, const N: usize>(
    mut backend: Cursor,
) -> Result<([T; N], Cursor), DeserializeError> {
    backend = T::pad_align_and_check(backend)?;
    let mut res = MaybeUninit::<[T; N]>::uninit();
    unsafe {
        for item in &mut res.assume_init_mut().iter_mut() {
            let (elem, new_backend) = T::_deserialize_full_copy_inner(backend)?;
            std::ptr::write(item, elem);
            backend = new_backend;
        }
        Ok((res.assume_init(), backend))
    }
}

mod private {
    use super::*;

    // This delegates to a private helper trait which we can specialize on in stable rust
    impl<T: CopyType + DeserializeInner + 'static, const N: usize> DeserializeInner for [T; N]
    where
        [T; N]: DeserializeHelper<<T as CopyType>::Copy, FullType = [T; N]>,
    {
        type DeserType<'a> = <[T; N] as DeserializeHelper<<T as CopyType>::Copy>>::DeserType<'a>;
        #[inline(always)]
        fn _deserialize_full_copy_inner(
            backend: Cursor,
        ) -> Result<([T; N], Cursor), DeserializeError> {
            <[T; N] as DeserializeHelper<<T as CopyType>::Copy>>::_deserialize_full_copy_inner_impl(
                backend,
            )
        }

        #[inline(always)]
        fn _deserialize_eps_copy_inner(
            backend: Cursor,
        ) -> Result<
            (
                <[T; N] as DeserializeHelper<<T as CopyType>::Copy>>::DeserType<'_>,
                Cursor,
            ),
            DeserializeError,
        > {
            <[T; N] as DeserializeHelper<<T as CopyType>::Copy>>::_deserialize_eps_copy_inner_impl(
                backend,
            )
        }
    }

    impl<T: ZeroCopy + DeserializeInner + 'static, const N: usize> DeserializeHelper<Zero> for [T; N] {
        type FullType = Self;
        type DeserType<'a> = &'a [T; N];
        #[inline(always)]
        fn _deserialize_full_copy_inner_impl(
            backend: Cursor,
        ) -> Result<([T; N], Cursor), DeserializeError> {
            deserialize_array_full(backend)
        }
        #[inline(always)]
        fn _deserialize_eps_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(<Self as DeserializeInner>::DeserType<'_>, Cursor), DeserializeError> {
            deserialize_array_zero(backend)
        }
    }

    impl<T: EpsCopy + DeserializeInner + 'static, const N: usize> DeserializeHelper<Eps> for [T; N] {
        type FullType = Self;
        type DeserType<'a> = [<T as DeserializeInner>::DeserType<'a>; N];
        #[inline(always)]
        fn _deserialize_full_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(Self, Cursor), DeserializeError> {
            deserialize_array_full(backend)
        }
        #[inline(always)]
        fn _deserialize_eps_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(<Self as DeserializeInner>::DeserType<'_>, Cursor), DeserializeError> {
            deserialize_array_eps::<T, N>(backend)
        }
    }

    fn deserialize_vec_eps<T: DeserializeInner>(
        backend: Cursor,
    ) -> Result<(Vec<<T as DeserializeInner>::DeserType<'_>>, Cursor), DeserializeError> {
        let (len, mut backend) = usize::_deserialize_full_copy_inner(backend)?;
        let mut res = Vec::with_capacity(len);
        for _ in 0..len {
            let (elem, new_backend) = T::_deserialize_eps_copy_inner(backend)?;
            res.push(elem);
            backend = new_backend;
        }
        Ok((res, backend))
    }

    fn deserialize_vec_full<T: DeserializeInner>(
        backend: Cursor,
    ) -> Result<(Vec<T>, Cursor), DeserializeError> {
        let (len, mut backend) = usize::_deserialize_full_copy_inner(backend)?;
        let mut res = Vec::with_capacity(len);
        for _ in 0..len {
            let (elem, new_backend) = T::_deserialize_full_copy_inner(backend)?;
            res.push(elem);
            backend = new_backend;
        }
        Ok((res, backend))
    }

    // Since impls with distinct parameters are considered disjoint
    // we can write multiple blanket impls for DeserializeHelper given different paremeters
    pub trait DeserializeHelper<T: CopySelector> {
        // TODO: do we really need this?
        type FullType: TypeHash;
        type DeserType<'a>: TypeHash;
        fn _deserialize_full_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(Self::FullType, Cursor), DeserializeError>;
        fn _deserialize_eps_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(Self::DeserType<'_>, Cursor), DeserializeError>;
    }

    // This delegates to a private helper trait which we can specialize on in stable rust
    impl<T: CopyType + DeserializeInner + 'static> DeserializeInner for Vec<T>
    where
        Vec<T>: DeserializeHelper<<T as CopyType>::Copy, FullType = Vec<T>>,
    {
        type DeserType<'a> = <Vec<T> as DeserializeHelper<<T as CopyType>::Copy>>::DeserType<'a>;
        #[inline(always)]
        fn _deserialize_full_copy_inner(
            backend: Cursor,
        ) -> Result<(Vec<T>, Cursor), DeserializeError> {
            <Vec<T> as DeserializeHelper<<T as CopyType>::Copy>>::_deserialize_full_copy_inner_impl(
                backend,
            )
        }

        #[inline(always)]
        fn _deserialize_eps_copy_inner(
            backend: Cursor,
        ) -> Result<
            (
                <Vec<T> as DeserializeHelper<<T as CopyType>::Copy>>::DeserType<'_>,
                Cursor,
            ),
            DeserializeError,
        > {
            <Vec<T> as DeserializeHelper<<T as CopyType>::Copy>>::_deserialize_eps_copy_inner_impl(
                backend,
            )
        }
    }

    impl<T: ZeroCopy + DeserializeInner + 'static> DeserializeHelper<Zero> for Vec<T> {
        type FullType = Self;
        type DeserType<'a> = &'a [T];
        #[inline(always)]
        fn _deserialize_full_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(Vec<T>, Cursor), DeserializeError> {
            deserialize_vec_full(backend)
        }
        #[inline(always)]
        fn _deserialize_eps_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(<Self as DeserializeInner>::DeserType<'_>, Cursor), DeserializeError> {
            deserialize_slice(backend)
        }
    }

    impl<T: EpsCopy + DeserializeInner + 'static> DeserializeHelper<Eps> for Vec<T> {
        type FullType = Self;
        type DeserType<'a> = Vec<<T as DeserializeInner>::DeserType<'a>>;
        #[inline(always)]
        fn _deserialize_full_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(Self, Cursor), DeserializeError> {
            deserialize_vec_full(backend)
        }
        #[inline(always)]
        fn _deserialize_eps_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(<Self as DeserializeInner>::DeserType<'_>, Cursor), DeserializeError> {
            deserialize_vec_eps::<T>(backend)
        }
    }

    // This delegates to a private helper trait which we can specialize on in stable rust
    impl<T: CopyType + DeserializeInner + 'static> DeserializeInner for Box<[T]>
    where
        Box<[T]>: DeserializeHelper<<T as CopyType>::Copy, FullType = Box<[T]>>,
    {
        type DeserType<'a> = <Box<[T]> as DeserializeHelper<<T as CopyType>::Copy>>::DeserType<'a>;
        #[inline(always)]
        fn _deserialize_full_copy_inner(
            backend: Cursor,
        ) -> Result<(Box<[T]>, Cursor), DeserializeError> {
            <Box<[T]> as DeserializeHelper<<T as CopyType>::Copy>>::_deserialize_full_copy_inner_impl(
            backend,
        )
        }

        #[inline(always)]
        fn _deserialize_eps_copy_inner(
            backend: Cursor,
        ) -> Result<
            (
                <Box<[T]> as DeserializeHelper<<T as CopyType>::Copy>>::DeserType<'_>,
                Cursor,
            ),
            DeserializeError,
        > {
            <Box<[T]> as DeserializeHelper<<T as CopyType>::Copy>>::_deserialize_eps_copy_inner_impl(
                backend,
            )
        }
    }

    impl<T: ZeroCopy + DeserializeInner + 'static> DeserializeHelper<Zero> for Box<[T]> {
        type FullType = Self;
        type DeserType<'a> = &'a [T];
        #[inline(always)]
        fn _deserialize_full_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(Box<[T]>, Cursor), DeserializeError> {
            deserialize_vec_full(backend).map(|(v, b)| (v.into_boxed_slice(), b))
        }
        #[inline(always)]
        fn _deserialize_eps_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(<Self as DeserializeInner>::DeserType<'_>, Cursor), DeserializeError> {
            deserialize_slice(backend)
        }
    }

    impl<T: EpsCopy + DeserializeInner + 'static> DeserializeHelper<Eps> for Box<[T]> {
        type FullType = Self;
        type DeserType<'a> = Box<[<T as DeserializeInner>::DeserType<'a>]>;
        #[inline(always)]
        fn _deserialize_full_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(Self, Cursor), DeserializeError> {
            deserialize_vec_full(backend).map(|(v, b)| (v.into_boxed_slice(), b))
        }
        #[inline(always)]
        fn _deserialize_eps_copy_inner_impl(
            backend: Cursor,
        ) -> Result<(<Self as DeserializeInner>::DeserType<'_>, Cursor), DeserializeError> {
            deserialize_vec_eps::<T>(backend).map(|(v, b)| (v.into_boxed_slice(), b))
        }
    }
}

impl DeserializeInner for String {
    fn _deserialize_full_copy_inner(backend: Cursor) -> Result<(Self, Cursor), DeserializeError> {
        let (slice, backend) = deserialize_slice(backend)?;
        let res = String::from_utf8(slice.to_vec()).unwrap();
        Ok((res, backend))
    }
    type DeserType<'a> = &'a str;
    #[inline(always)]
    fn _deserialize_eps_copy_inner(
        backend: Cursor,
    ) -> Result<(Self::DeserType<'_>, Cursor), DeserializeError> {
        let (slice, backend) = deserialize_slice(backend)?;
        dbg!(slice.len());
        Ok((
            unsafe {
                #[allow(clippy::transmute_bytes_to_str)]
                core::mem::transmute::<&'_ [u8], &'_ str>(slice)
            },
            backend,
        ))
    }
}

impl DeserializeInner for Box<str> {
    #[inline(always)]
    fn _deserialize_full_copy_inner(backend: Cursor) -> Result<(Self, Cursor), DeserializeError> {
        String::_deserialize_full_copy_inner(backend).map(|(d, a)| (d.into_boxed_str(), a))
    }
    type DeserType<'a> = &'a str;
    #[inline(always)]
    fn _deserialize_eps_copy_inner(
        backend: Cursor,
    ) -> Result<(Self::DeserType<'_>, Cursor), DeserializeError> {
        let (slice, backend) = deserialize_slice(backend)?;
        Ok((
            #[allow(clippy::transmute_bytes_to_str)]
            unsafe {
                core::mem::transmute::<&'_ [u8], &'_ str>(slice)
            },
            backend,
        ))
    }
}