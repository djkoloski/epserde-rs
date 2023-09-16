/*
 * SPDX-FileCopyrightText: 2023 Inria
 * SPDX-FileCopyrightText: 2023 Sebastiano Vigna
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

/*!

Implementations for vectors.

*/
use crate::des;
use crate::des::*;
use crate::ser;
use crate::ser::*;
use crate::traits::*;
use core::hash::Hash;

impl<T> CopyType for Vec<T> {
    type Copy = Deep;
}

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::vec::Vec;
#[cfg(feature = "alloc")]
impl<T: TypeHash> TypeHash for Vec<T> {
    fn type_hash(
        type_hasher: &mut impl core::hash::Hasher,
        repr_hasher: &mut impl core::hash::Hasher,
        _offset_of: &mut usize,
    ) {
        "Vec".hash(type_hasher);
        T::type_hash(type_hasher, repr_hasher, _offset_of);
    }
}

impl<T: CopyType + SerializeInner + TypeHash> SerializeInner for Vec<T>
where
    Vec<T>: SerializeHelper<<T as CopyType>::Copy>,
{
    const IS_ZERO_COPY: bool = false;
    const ZERO_COPY_MISMATCH: bool = false;
    fn _serialize_inner(&self, backend: &mut impl FieldWrite) -> ser::Result<()> {
        SerializeHelper::_serialize_inner(self, backend)
    }
}

impl<T: ZeroCopy + SerializeInner> SerializeHelper<Zero> for Vec<T> {
    #[inline(always)]
    fn _serialize_inner(&self, backend: &mut impl FieldWrite) -> ser::Result<()> {
        backend.write_slice_zero(self.as_slice())
    }
}

impl<T: DeepCopy + SerializeInner> SerializeHelper<Deep> for Vec<T> {
    #[inline(always)]
    fn _serialize_inner(&self, backend: &mut impl FieldWrite) -> ser::Result<()> {
        backend.write_slice(self.as_slice())
    }
}

// This delegates to a private helper trait which we can specialize on in stable rust
impl<T: CopyType + DeserializeInner + 'static> DeserializeInner for Vec<T>
where
    Vec<T>: DeserializeHelper<<T as CopyType>::Copy, FullType = Vec<T>>,
{
    type DeserType<'a> = <Vec<T> as DeserializeHelper<<T as CopyType>::Copy>>::DeserType<'a>;
    #[inline(always)]
    fn _deserialize_full_copy_inner(backend: &mut impl ReadWithPos) -> des::Result<Self> {
        <Vec<T> as DeserializeHelper<<T as CopyType>::Copy>>::_deserialize_full_copy_inner_impl(
            backend,
        )
    }

    #[inline(always)]
    fn _deserialize_eps_copy_inner<'a>(
        backend: &mut SliceWithPos<'a>,
    ) -> des::Result<<Vec<T> as DeserializeHelper<<T as CopyType>::Copy>>::DeserType<'a>> {
        <Vec<T> as DeserializeHelper<<T as CopyType>::Copy>>::_deserialize_eps_copy_inner_impl(
            backend,
        )
    }
}

impl<T: ZeroCopy + DeserializeInner + 'static> DeserializeHelper<Zero> for Vec<T> {
    type FullType = Self;
    type DeserType<'a> = &'a [T];
    #[inline(always)]
    fn _deserialize_full_copy_inner_impl(backend: &mut impl ReadWithPos) -> des::Result<Self> {
        backend.deserialize_vec_full_zero()
    }
    #[inline(always)]
    fn _deserialize_eps_copy_inner_impl<'a>(
        backend: &mut SliceWithPos<'a>,
    ) -> des::Result<<Self as DeserializeInner>::DeserType<'a>> {
        backend.deserialize_slice_zero()
    }
}

impl<T: DeepCopy + DeserializeInner + 'static> DeserializeHelper<Deep> for Vec<T> {
    type FullType = Self;
    type DeserType<'a> = Vec<<T as DeserializeInner>::DeserType<'a>>;
    #[inline(always)]
    fn _deserialize_full_copy_inner_impl(backend: &mut impl ReadWithPos) -> des::Result<Self> {
        backend.deserialize_vec_full_eps()
    }
    #[inline(always)]
    fn _deserialize_eps_copy_inner_impl<'a>(
        backend: &mut SliceWithPos<'a>,
    ) -> des::Result<<Self as DeserializeInner>::DeserType<'a>> {
        backend.deserialize_vec_eps_eps::<T>()
    }
}
