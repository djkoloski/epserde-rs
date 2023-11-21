/*
 * SPDX-FileCopyrightText: 2023 Inria
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

#![cfg(test)]

use std::iter;

use epserde::prelude::*;

macro_rules! impl_test {
    ($ty:ty, $val:expr) => {{
        let a = $val;
        let mut buf = epserde::new_aligned_cursor();

        let mut schema = a.serialize_with_schema(&mut buf).unwrap();
        schema.0.sort_by_key(|a| a.offset);
        println!("{}", schema.to_csv());

        buf.set_position(0);
        let a1 = <$ty>::deserialize_full(&mut buf).unwrap();
        assert_eq!(a, a1);

        buf.set_position(0);
        let bytes = buf.into_inner();
        let a2 = <$ty>::deserialize_eps(&bytes).unwrap();
        assert_eq!(a, a2);
    }};
}

#[test]
fn test_array_usize() {
    let a = [1, 2, 3, 4, 5];

    let mut buf = epserde::new_aligned_cursor();

    let mut schema = a.serialize_with_schema(&mut buf).unwrap();
    schema.0.sort_by_key(|a| a.offset);
    println!("{}", schema.to_csv());

    buf.set_position(0);
    let a1 = <[usize; 5]>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, a1);

    buf.set_position(0);
    let bytes = buf.into_inner();
    let a2 = <[usize; 5]>::deserialize_eps(&bytes).unwrap();
    assert_eq!(a, *a2);
}

#[test]
fn test_vec_usize() {
    impl_test!(Vec<usize>, vec![1, 2, 3, 4, 5])
}

#[test]
fn test_box_slice_usize() {
    let a = vec![1, 2, 3, 4, 5].into_boxed_slice();

    let mut buf = epserde::new_aligned_cursor();

    let mut schema = a.serialize_with_schema(&mut buf).unwrap();
    schema.0.sort_by_key(|a| a.offset);
    println!("{}", schema.to_csv());

    buf.set_position(0);
    let a1 = <Box<[usize]>>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, a1);

    buf.set_position(0);
    let bytes = buf.into_inner();
    let a2 = <Box<[usize]>>::deserialize_eps(&bytes).unwrap();
    assert_eq!(a, a2.into());
}

#[test]
fn test_box_slice_string() {
    let a = vec!["A".to_string(), "V".to_string()].into_boxed_slice();

    let mut buf = epserde::new_aligned_cursor();

    let mut schema = a.serialize_with_schema(&mut buf).unwrap();
    schema.0.sort_by_key(|a| a.offset);
    println!("{}", schema.to_csv());

    buf.set_position(0);
    let a1 = <Box<[String]>>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, a1);

    buf.set_position(0);
    let bytes = buf.into_inner();
    let a2 = <Box<[String]>>::deserialize_eps(&bytes).unwrap();
    assert_eq!(a.len(), a2.len());
    a.iter().zip(a2.iter()).for_each(|(a, a2)| {
        assert_eq!(a, a2);
    });
}

#[test]
fn test_vec_vec_usize() {
    impl_test!(Vec<Vec<usize>>, vec![vec![1, 2, 3], vec![4, 5]])
}

#[test]
fn test_vec_array_string() {
    impl_test!(
        Vec<[String; 2]>,
        vec![
            ["a".to_string(), "b".to_string()],
            ["c".to_string(), "aasfihjasomk".to_string()]
        ]
    )
}

#[test]
fn test_vec_vec_string() {
    impl_test!(
        Vec<Vec<String>>,
        vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["c".to_string(), "aasfihjasomk".to_string()]
        ]
    )
}

#[test]
fn test_vec_vec_array_array_string() {
    impl_test!(
        Vec<Vec<[[String; 2]; 2]>>,
        vec![
            vec![[
                ["a".to_string(), "b".to_string()],
                ["c".to_string(), "d".to_string()],
            ]],
            vec![[
                ["a".to_string(), "b".to_string()],
                ["c".to_string(), "d".to_string()],
            ]],
        ]
    )
}

#[test]
fn test_vec_vec_array_array_usize() {
    impl_test!(
        Vec<Vec<[[usize; 2]; 2]>>,
        vec![vec![[[1, 2], [3, 4],]], vec![[[5, 6], [7, 8],]],]
    )
}

#[test]
fn test_struct_deep() {
    #[derive(Epserde, Copy, Clone, Debug, PartialEq)]
    struct Struct {
        a: usize,
        b: usize,
        c: i32,
    }
    let a = Struct { a: 0, b: 1, c: 2 };
    let mut buf = epserde::new_aligned_cursor();
    // Serialize
    let _bytes_written = a.serialize(&mut buf).unwrap();

    buf.set_position(0);
    let full = <Struct>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, full);

    buf.set_position(0);
    let eps = <Struct>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, eps);
}

#[test]
fn test_struct_zero() {
    #[derive(Epserde, Copy, Clone, Debug, PartialEq)]
    #[repr(C)]
    #[zero_copy]
    struct Struct {
        a: usize,
        b: usize,
        c: i32,
    }
    let a = Struct { a: 0, b: 1, c: 2 };
    let mut buf = epserde::new_aligned_cursor();
    // Serialize
    let _bytes_written = a.serialize(&mut buf).unwrap();

    buf.set_position(0);
    let full = <Struct>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, full);

    buf.set_position(0);
    let eps = <Struct>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, eps);
}

#[test]
fn test_tuple_struct_deep() {
    #[derive(Epserde, Copy, Clone, Debug, PartialEq)]
    struct Tuple(usize, usize, i32);
    let a = Tuple(0, 1, 2);
    let mut buf = epserde::new_aligned_cursor();
    // Serialize
    let _bytes_written = a.serialize(&mut buf).unwrap();

    buf.set_position(0);
    let full = <Tuple>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, full);

    buf.set_position(0);
    let eps = <Tuple>::deserialize_eps(&buf.into_inner()).unwrap();
    assert_eq!(a, eps);
}

#[test]
fn test_tuple_struct_zero() {
    #[derive(Epserde, Copy, Clone, Debug, PartialEq)]
    #[repr(C)]
    #[zero_copy]
    struct Tuple(usize, usize, i32);
    let a = Tuple(0, 1, 2);
    let mut buf = epserde::new_aligned_cursor();
    // Serialize
    let _bytes_written = a.serialize(&mut buf).unwrap();

    buf.set_position(0);
    let full = <Tuple>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, full);

    buf.set_position(0);
    let bytes = &buf.into_inner();
    let eps = <Tuple>::deserialize_eps(bytes).unwrap();
    assert_eq!(a, *eps);
}

#[test]
fn test_enum_deep() {
    #[derive(Epserde, Clone, Debug, PartialEq)]
    enum Data<V = Vec<usize>> {
        A,
        B(u64),
        C(u64, Vec<usize>),
        D { a: i32, b: V },
    }
}

#[test]
fn test_enum_zero() {
    #[derive(Epserde, Clone, Copy, Debug, PartialEq)]
    #[repr(C)]
    #[zero_copy]
    enum Data {
        A,
        B(u64),
        C(u64),
        D { a: i32, b: i32 },
    }

    let mut buf = epserde::new_aligned_cursor();
    let a = Data::A;
    a.serialize(&mut buf).unwrap();
    buf.set_position(0);
    let full = <Data>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, full);
    buf.set_position(0);
    let bytes = &buf.into_inner();
    let eps = <Data>::deserialize_eps(bytes).unwrap();
    assert_eq!(a, *eps);

    let mut buf = epserde::new_aligned_cursor();
    let a = Data::B(3);
    a.serialize(&mut buf).unwrap();
    buf.set_position(0);
    let full = <Data>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, full);
    buf.set_position(0);
    let bytes = &buf.into_inner();
    let eps = <Data>::deserialize_eps(bytes).unwrap();
    assert_eq!(a, *eps);

    let mut buf = epserde::new_aligned_cursor();
    let a = Data::C(4);
    a.serialize(&mut buf).unwrap();
    buf.set_position(0);
    let full = <Data>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, full);
    buf.set_position(0);
    let bytes = &buf.into_inner();
    let eps = <Data>::deserialize_eps(bytes).unwrap();
    assert_eq!(a, *eps);

    let mut buf = epserde::new_aligned_cursor();
    let a = Data::D { a: 1, b: 2 };
    a.serialize(&mut buf).unwrap();
    buf.set_position(0);
    let full = <Data>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, full);
    buf.set_position(0);
    let bytes = &buf.into_inner();
    let eps = <Data>::deserialize_eps(bytes).unwrap();
    assert_eq!(a, *eps);

    let mut buf = epserde::new_aligned_cursor();
    let a = Vec::from_iter(iter::repeat(Data::A).take(10));
    a.serialize(&mut buf).unwrap();
    buf.set_position(0);
    let full = <Vec<Data>>::deserialize_full(&mut buf).unwrap();
    assert_eq!(a, full);
    buf.set_position(0);
    let bytes = &buf.into_inner();
    let eps = <Vec<Data>>::deserialize_eps(bytes).unwrap();
    assert_eq!(a, *eps);
}
