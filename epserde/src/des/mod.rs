/*
 * SPDX-FileCopyrightText: 2023 Inria
 * SPDX-FileCopyrightText: 2023 Sebastiano Vigna
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

/*!

Deserialization traits and types

[`Deserialize`] is the main deserialization trait, providing methods
[`Deserialize::deserialize_eps_copy`] and [`Deserialize::deserialize_full_copy`]
which implement ε-copy and full-copy deserialization, respectively.
The implementation of this trait is based on [`DeserializeInner`],
which is automatically derived with `#[derive(Deserialize)]`.

*/

use crate::traits::*;
use crate::{MAGIC, MAGIC_REV, VERSION};
use core::ptr::addr_of_mut;
use core::{hash::Hasher, mem::MaybeUninit};
use std::{io::BufReader, path::Path};
pub mod read_with_pos;
pub use read_with_pos::*;

pub mod mem_case;
pub use mem_case::*;
pub mod slice_with_pos;
pub use slice_with_pos::*;

pub type Result<T> = core::result::Result<T, DeserializeError>;

/// Main deserialization trait. It is separated from [`DeserializeInner`] to
/// avoid that the user modify its behavior, and hide internal serialization
/// methods.
///
/// It provides several convenience methods to load or map into memory
/// structures that have been previously serialized. See, for example,
/// [`Deserialize::load_full`], [`Deserialize::load_mem`], and [`Deserialize::mmap`].
pub trait Deserialize: DeserializeInner {
    /// Fully deserialize a structure of this type from the given backend.
    fn deserialize_full_copy(backend: impl ReadNoStd) -> Result<Self>;
    /// ε-copy deserialize a structure of this type from the given backend.
    fn deserialize_eps_copy(backend: &'_ [u8]) -> Result<Self::DeserType<'_>>;

    /// Commodity method to fully deserialize from a file.
    fn load_full(path: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(DeserializeError::FileOpenError)?;
        let mut buf_reader = BufReader::new(file);
        Self::deserialize_full_copy(&mut buf_reader)
    }

    /// Load a file into heap-allocated memory and ε-deserialize a data structure from it,
    /// returning a [`MemCase`] containing the data structure and the
    /// memory. Excess bytes are zeroed out.
    fn load_mem<'a>(
        path: impl AsRef<Path>,
    ) -> anyhow::Result<MemCase<<Self as DeserializeInner>::DeserType<'a>>> {
        let file_len = path.as_ref().metadata()?.len() as usize;
        let mut file = std::fs::File::open(path)?;
        // Round up to u128 size
        let len = file_len + crate::pad_align_to(file_len, 16);

        let mut uninit: MaybeUninit<MemCase<<Self as DeserializeInner>::DeserType<'_>>> =
            MaybeUninit::uninit();
        let ptr = uninit.as_mut_ptr();

        // SAFETY: the entire vector will be filled with data read from the file,
        // or with zeroes if the file is shorter than the vector.
        let mut bytes = unsafe {
            Vec::from_raw_parts(
                std::alloc::alloc(std::alloc::Layout::from_size_align(len, 16)?),
                len,
                len,
            )
        };

        file.read_exact(&mut bytes[..file_len])?;
        // Fixes the last few bytes to guarantee zero-extension semantics
        // for bit vectors and full-vector initialization.
        bytes[file_len..].fill(0);
        let backend = MemBackend::Memory(bytes);

        // store the backend inside the MemCase
        unsafe {
            addr_of_mut!((*ptr).1).write(backend);
        }
        // deserialize the data structure
        let mem = unsafe { (*ptr).1.as_ref().unwrap() };
        let s = Self::deserialize_eps_copy(mem)?;
        // write the deserialized struct in the memcase
        unsafe {
            addr_of_mut!((*ptr).0).write(s);
        }
        // finish init
        Ok(unsafe { uninit.assume_init() })
    }

    /// Load a file into `mmap()`-allocated memory and ε-deserialize a data structure from it,
    /// returning a [`MemCase`] containing the data structure and the
    /// memory. Excess bytes are zeroed out.
    ///
    /// The behavior of `mmap()` can be modified by passing some [`Flags`]; otherwise,
    /// just pass `Flags::empty()`.
    #[allow(clippy::uninit_vec)]
    fn load_mmap<'a>(
        path: impl AsRef<Path>,
        flags: Flags,
    ) -> anyhow::Result<MemCase<<Self as DeserializeInner>::DeserType<'a>>> {
        let file_len = path.as_ref().metadata()?.len() as usize;
        let mut file = std::fs::File::open(path)?;
        let capacity = (file_len + 7) / 8;

        let mut uninit: MaybeUninit<MemCase<<Self as DeserializeInner>::DeserType<'_>>> =
            MaybeUninit::uninit();
        let ptr = uninit.as_mut_ptr();

        let mut mmap = mmap_rs::MmapOptions::new(capacity * 8)?
            .with_flags(flags.mmap_flags())
            .map_mut()?;
        file.read_exact(&mut mmap[..file_len])?;
        // Fixes the last few bytes to guarantee zero-extension semantics
        // for bit vectors.
        mmap[file_len..].fill(0);

        let backend = MemBackend::Mmap(mmap.make_read_only().map_err(|(_, err)| err).unwrap());

        // store the backend inside the MemCase
        unsafe {
            addr_of_mut!((*ptr).1).write(backend);
        }
        // deserialize the data structure
        let mem = unsafe { (*ptr).1.as_ref().unwrap() };
        let s = Self::deserialize_eps_copy(mem)?;
        // write the deserialized struct in the MemCase
        unsafe {
            addr_of_mut!((*ptr).0).write(s);
        }
        // finish init
        Ok(unsafe { uninit.assume_init() })
    }

    /// Memory map a file and ε-deserialize a data structure from it,
    /// returning a [`MemCase`] containing the data structure and the
    /// memory mapping.
    ///
    /// The behavior of `mmap()` can be modified by passing some [`Flags`]; otherwise,
    /// just pass `Flags::empty()`.
    #[allow(clippy::uninit_vec)]
    fn mmap<'a>(
        path: impl AsRef<Path>,
        flags: Flags,
    ) -> anyhow::Result<MemCase<<Self as DeserializeInner>::DeserType<'a>>> {
        let file_len = path.as_ref().metadata()?.len();
        let file = std::fs::File::open(path)?;

        let mut uninit: MaybeUninit<MemCase<<Self as DeserializeInner>::DeserType<'_>>> =
            MaybeUninit::uninit();
        let ptr = uninit.as_mut_ptr();

        let mmap = unsafe {
            mmap_rs::MmapOptions::new(file_len as _)?
                .with_flags(flags.mmap_flags())
                .with_file(file, 0)
                .map()?
        };

        // store the backend inside the MemCase
        unsafe {
            addr_of_mut!((*ptr).1).write(MemBackend::Mmap(mmap));
        }

        let mmap = unsafe { (*ptr).1.as_ref().unwrap() };
        // deserialize the data structure
        let s = Self::deserialize_eps_copy(mmap)?;
        // write the deserialized struct in the MemCase
        unsafe {
            addr_of_mut!((*ptr).0).write(s);
        }
        // finish init
        Ok(unsafe { uninit.assume_init() })
    }
}

/// Blanket implementation that prevents the user from overwriting the
/// methods in [`Deserialize`].
///
/// This implementation [checks the header](`check_header`) written
/// by the blanket implementation of [`crate::ser::Serialize`] and then calls
/// [`DeserializeInner::_deserialize_full_copy_inner`] or
/// [`DeserializeInner::_deserialize_eps_copy_inner`].

impl<T: DeserializeInner> Deserialize for T {
    fn deserialize_full_copy(backend: impl ReadNoStd) -> Result<Self> {
        let mut backend = ReaderWithPos::new(backend);

        let mut hasher = xxhash_rust::xxh3::Xxh3::new();
        Self::type_hash(&mut hasher);
        let self_hash = hasher.finish();
        let mut hasher = xxhash_rust::xxh3::Xxh3::new();
        Self::type_repr_hash(&mut hasher);
        let self_repr_hash = hasher.finish();

        backend = check_header(
            backend,
            self_hash,
            self_repr_hash,
            core::any::type_name::<Self>().to_string(),
        )?;
        let (res, _) = Self::_deserialize_full_copy_inner(backend)?;
        Ok(res)
    }

    fn deserialize_eps_copy(backend: &'_ [u8]) -> Result<Self::DeserType<'_>> {
        let mut backend = SliceWithPos::new(backend);

        let mut hasher = xxhash_rust::xxh3::Xxh3::new();
        Self::type_hash(&mut hasher);
        let self_hash = hasher.finish();
        let mut hasher = xxhash_rust::xxh3::Xxh3::new();
        Self::type_repr_hash(&mut hasher);
        let self_repr_hash = hasher.finish();

        backend = check_header(
            backend,
            self_hash,
            self_repr_hash,
            core::any::type_name::<Self>().to_string(),
        )?;
        let (res, _) = Self::_deserialize_eps_copy_inner(backend)?;
        Ok(res)
    }
}

/// Inner trait to implement deserialization of a type. This trait exists
/// to separate the user-facing [`Deserialize`] trait from the low-level
/// deserialization mechanisms of [`DeserializeInner::_deserialize_full_copy_inner`]
/// and [`DeserializeInner::_deserialize_eps_copy_inner`]. Moreover,
/// it makes it possible to behave slighly differently at the top
/// of the recursion tree (e.g., to check the endianness marker), and to prevent
/// the user from modifying the methods in [`Deserialize`].
///
/// The user should not implement this trait directly, but rather derive it.
pub trait DeserializeInner: TypeHash + Sized {
    type DeserType<'a>;
    fn _deserialize_full_copy_inner<R: ReadWithPos>(backend: R) -> Result<(Self, R)>;

    fn _deserialize_eps_copy_inner(
        backend: SliceWithPos,
    ) -> Result<(Self::DeserType<'_>, SliceWithPos)>;
}

/// Common code for both full-copy and zero-copy deserialization
/// Must be kept in sync with [`crate::ser::write_header`].
pub fn check_header<R: ReadWithPos>(
    backend: R,
    self_hash: u64,
    self_repr_hash: u64,
    self_name: String,
) -> Result<R> {
    let (magic, backend) = u64::_deserialize_full_copy_inner(backend)?;
    match magic {
        MAGIC => Ok(()),
        MAGIC_REV => Err(DeserializeError::EndiannessError),
        magic => Err(DeserializeError::MagicCookieError(magic)),
    }?;

    let (major, backend) = u16::_deserialize_full_copy_inner(backend)?;
    if major != VERSION.0 {
        return Err(DeserializeError::MajorVersionMismatch(major));
    }
    let (minor, backend) = u16::_deserialize_full_copy_inner(backend)?;
    if minor > VERSION.1 {
        return Err(DeserializeError::MinorVersionMismatch(minor));
    };

    let (usize_size, backend) = u8::_deserialize_full_copy_inner(backend)?;
    let usize_size = usize_size as usize;
    let native_usize_size = core::mem::size_of::<usize>();
    if usize_size != native_usize_size {
        return Err(DeserializeError::UsizeSizeMismatch(usize_size));
    };

    let (type_hash, backend) = u64::_deserialize_full_copy_inner(backend)?;
    let (type_repr_hash, backend) = u64::_deserialize_full_copy_inner(backend)?;
    let (type_name, backend) = String::_deserialize_full_copy_inner(backend)?;

    if type_hash != self_hash {
        return Err(DeserializeError::WrongTypeHash {
            got_type_name: self_name,
            got: self_hash,
            expected_type_name: type_name,
            expected: type_hash,
        });
    }
    if type_repr_hash != self_repr_hash {
        return Err(DeserializeError::WrongTypeReprHash {
            got_type_name: self_name,
            got: self_repr_hash,
            expected_type_name: type_name,
            expected: type_repr_hash,
        });
    }

    Ok(backend)
}

/// A helper trait that makes it possible to implement differently
/// deserialization for [`crate::traits::ZeroCopy`] and [`crate::traits::FullCopy`] types.
/// See [`crate::traits::CopyType`] for more information.
pub trait DeserializeHelper<T: CopySelector> {
    // TODO: do we really need this?
    type FullType: TypeHash;
    type DeserType<'a>;
    fn _deserialize_full_copy_inner_impl<R: ReadWithPos>(backend: R)
        -> Result<(Self::FullType, R)>;
    fn _deserialize_eps_copy_inner_impl(
        backend: SliceWithPos,
    ) -> Result<(Self::DeserType<'_>, SliceWithPos)>;
}

#[derive(Debug)]
/// Errors that can happen during deserialization.
pub enum DeserializeError {
    /// [`Deserialize::load_full`] could not open the provided file.
    FileOpenError(std::io::Error),
    /// The underlying reader returned an error.
    ReadError,
    /// The file is reasonable but the endianess is wrong.
    EndiannessError,
    /// Some field is not properly aligned.
    AlignmentError,
    /// The file was serialized with a version of ε-serde that is not compatible.
    MajorVersionMismatch(u16),
    /// The file was serialized with a compatible, but too new version of ε-serde
    /// so we might be missing features.
    MinorVersionMismatch(u16),
    /// The the `pointer_width` of the serialized file is different from the
    /// `pointer_width` of the current architecture.
    /// For example, the file was serialized on a 64-bit machine and we are trying to
    /// deserialize it on a 32-bit machine.
    /// We could check if the usizes are actually used, but currently we do not.
    UsizeSizeMismatch(usize),
    /// The magic coookie is wrong. The byte sequence is not an ε-serde serialization.
    MagicCookieError(u64),
    /// A tag is wrong (e.g., for [`Option`]).
    InvalidTag(u8),
    /// The type hash is wrong. Probably the user is trying to deserialize a
    /// file with the wrong type.
    WrongTypeHash {
        got_type_name: String,
        expected_type_name: String,
        expected: u64,
        got: u64,
    },
    /// The type representation hash is wrong. Probabliy the user is trying to
    /// deserialize a file with the right type but this type has different
    /// in-memory representations on the serialization arch and on the current one.
    WrongTypeReprHash {
        got_type_name: String,
        expected_type_name: String,
        expected: u64,
        got: u64,
    },
}

impl std::error::Error for DeserializeError {}

impl core::fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::ReadError => write!(f, "Read error during ε-serde serialization"),
            Self::FileOpenError(error) => {
                write!(f, "Write error during ε-serde serialization: {}", error)
            }
            Self::EndiannessError => write!(
                f,
                "The current arch is {}-endian but the data is {}-endian.",
                if cfg!(target_endian = "little") {
                    "little"
                } else {
                    "big"
                },
                if cfg!(target_endian = "little") {
                    "big"
                } else {
                    "little"
                }
            ),
            Self::MagicCookieError(magic) => write!(
                f,
                "Wrong magic cookie {:#018x}. The byte stream is not ε-serde serialization.",
                magic,
            ),
            Self::MajorVersionMismatch(found_major) => write!(
                f,
                "Major version mismatch. Expected {} but got {}.",
                VERSION.0, found_major,
            ),
            Self::MinorVersionMismatch(found_minor) => write!(
                f,
                "Minor version mismatch. Expected {} but got {}.",
                VERSION.1, found_minor,
            ),
            Self::UsizeSizeMismatch(usize_size) => write!(
                f,
                "The file was serialized on a machine where an usize has size {}, but on the current machine it has size {}.",
                usize_size,
                core::mem::size_of::<usize>()
            ),
            Self::AlignmentError => write!(f, "Alignment error. Most likely you are deserializing from a memory region with insufficient alignment."),
            Self::InvalidTag(tag) => write!(f, "Invalid tag: 0x{:02x}", tag),
            Self::WrongTypeHash {
                got_type_name,
                expected_type_name,
                expected,
                got,
            } => {
                write!(
                    f,
                    concat!(
                        "Wrong type hash. Expected: 0x{:#018x} Actual: {:#018x}.\n",
                        "The serialized type is '{}' and the deserialized type is '{}'.",
                    ),
                    expected, got, expected_type_name, got_type_name,
                )
            },
            Self::WrongTypeReprHash {
                got_type_name,
                expected_type_name,
                expected,
                got,
            } => {
                write!(
                    f,
                    concat!(
                        "Wrong type repr hash. Expected: 0x{:#018x} Actual: {:#018x}.\n",
                        "You might be trying to deserialize a file that was serialized on ",
                        "an architecture with different alignment requirements, or some ",
                        "of the fields of the type have changed their copy type (zero or not).\n",
                        "The serialized type is '{}' and the deserialized type is '{}'.",
                    ),
                    expected, got, expected_type_name, got_type_name,
                )
            }
        }
    }
}
