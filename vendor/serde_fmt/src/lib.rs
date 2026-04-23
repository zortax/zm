//! Vendored `serde_fmt` with the `From<Error> for fmt::Error` impl removed
//! to prevent ambiguity with stylo's `ToCss` derive macro.

#![cfg_attr(not(test), no_std)]

#[cfg(all(not(test), not(feature = "std")))]
extern crate core as std;

#[cfg(any(test, feature = "std"))]
extern crate std;

use crate::std::fmt::{self, Debug, Display};

use serde_core::ser::{
    self, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant, Serializer,
};

pub fn to_writer(v: impl Serialize, mut w: impl fmt::Write) -> fmt::Result {
    w.write_fmt(format_args!("{:?}", to_debug(v)))
}

pub fn to_debug<T>(v: T) -> ToDebug<T>
where
    T: Serialize,
{
    ToDebug(v)
}

#[derive(Clone, Copy)]
pub struct ToDebug<T>(T);

impl<T> Debug for ToDebug<T>
where
    T: Serialize,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl<T> Display for ToDebug<T>
where
    T: Serialize,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0.serialize(Formatter::new(f)) {
            Ok(()) => Ok(()),
            Err(e) => write!(f, "<{}>", e),
        }
    }
}

impl<T> Serialize for ToDebug<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

struct Formatter<'a, 'b: 'a>(&'a mut fmt::Formatter<'b>);

impl<'a, 'b: 'a> Formatter<'a, 'b> {
    fn new(fmt: &'a mut fmt::Formatter<'b>) -> Self {
        Formatter(fmt)
    }

    fn fmt(self, v: impl Debug) -> Result<(), Error> {
        v.fmt(self.0).map_err(|_| Error)
    }
}

impl<'a, 'b: 'a> Serializer for Formatter<'a, 'b> {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = DebugSeq<'a, 'b>;
    type SerializeTuple = DebugTuple<'a, 'b>;
    type SerializeTupleStruct = DebugTupleStruct<'a, 'b>;
    type SerializeTupleVariant = DebugTupleVariant<'a, 'b>;
    type SerializeMap = DebugMap<'a, 'b>;
    type SerializeStruct = DebugStruct<'a, 'b>;
    type SerializeStructVariant = DebugStructVariant<'a, 'b>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> { self.fmt(v) }
    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> { self.fmt(v) }

    fn collect_str<T: ?Sized>(self, v: &T) -> Result<Self::Ok, Self::Error>
    where T: Display {
        self.fmt(format_args!("{}", v))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> { self.fmt(v) }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        write!(self.0, "None").map_err(|_| Error)
    }

    fn serialize_some<T>(self, v: &T) -> Result<Self::Ok, Self::Error>
    where T: ?Sized + Serialize {
        self.serialize_newtype_struct("Some", v)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        write!(self.0, "()").map_err(|_| Error)
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serialize_tuple_struct(name, 0)?.end()
    }

    fn serialize_unit_variant(self, _name: &'static str, _variant_index: u32, variant: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serialize_tuple_struct(variant, 0)?.end()
    }

    fn serialize_newtype_struct<T>(self, name: &'static str, v: &T) -> Result<Self::Ok, Self::Error>
    where T: ?Sized + Serialize {
        let mut tuple = self.serialize_tuple_struct(name, 1)?;
        tuple.serialize_field(v)?;
        tuple.end()
    }

    fn serialize_newtype_variant<T>(self, _name: &'static str, _variant_index: u32, variant: &'static str, v: &T) -> Result<Self::Ok, Self::Error>
    where T: ?Sized + Serialize {
        let mut tuple = self.serialize_tuple_struct(variant, 1)?;
        tuple.serialize_field(v)?;
        tuple.end()
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(DebugSeq(self.0.debug_list()))
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(DebugTuple(self.0.debug_tuple("")))
    }

    fn serialize_tuple_struct(self, name: &'static str, _len: usize) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(DebugTupleStruct(self.0.debug_tuple(name)))
    }

    fn serialize_tuple_variant(self, _name: &'static str, _variant_index: u32, variant: &'static str, _len: usize) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(DebugTupleVariant(self.0.debug_tuple(variant)))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(DebugMap(self.0.debug_map()))
    }

    fn serialize_struct(self, name: &'static str, _len: usize) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(DebugStruct(self.0.debug_struct(name)))
    }

    fn serialize_struct_variant(self, _name: &'static str, _variant_index: u32, variant: &'static str, _len: usize) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(DebugStructVariant(self.0.debug_struct(variant)))
    }
}

struct DebugSeq<'a, 'b: 'a>(fmt::DebugList<'a, 'b>);
impl<'a, 'b: 'a> SerializeSeq for DebugSeq<'a, 'b> {
    type Ok = (); type Error = Error;
    fn serialize_element<T>(&mut self, v: &T) -> Result<(), Error> where T: ?Sized + Serialize {
        self.0.entry(&to_debug(v)); Ok(())
    }
    fn end(mut self) -> Result<(), Error> { self.0.finish().map_err(|_| Error) }
}

struct DebugTuple<'a, 'b: 'a>(fmt::DebugTuple<'a, 'b>);
impl<'a, 'b: 'a> SerializeTuple for DebugTuple<'a, 'b> {
    type Ok = (); type Error = Error;
    fn serialize_element<T>(&mut self, v: &T) -> Result<(), Error> where T: ?Sized + Serialize {
        self.0.field(&to_debug(v)); Ok(())
    }
    fn end(mut self) -> Result<(), Error> { self.0.finish().map_err(|_| Error) }
}

struct DebugTupleStruct<'a, 'b: 'a>(fmt::DebugTuple<'a, 'b>);
impl<'a, 'b: 'a> SerializeTupleStruct for DebugTupleStruct<'a, 'b> {
    type Ok = (); type Error = Error;
    fn serialize_field<T>(&mut self, v: &T) -> Result<(), Error> where T: ?Sized + Serialize {
        self.0.field(&to_debug(v)); Ok(())
    }
    fn end(mut self) -> Result<(), Error> { self.0.finish().map_err(|_| Error) }
}

struct DebugTupleVariant<'a, 'b: 'a>(fmt::DebugTuple<'a, 'b>);
impl<'a, 'b: 'a> SerializeTupleVariant for DebugTupleVariant<'a, 'b> {
    type Ok = (); type Error = Error;
    fn serialize_field<T>(&mut self, v: &T) -> Result<(), Error> where T: ?Sized + Serialize {
        self.0.field(&to_debug(v)); Ok(())
    }
    fn end(mut self) -> Result<(), Error> { self.0.finish().map_err(|_| Error) }
}

struct DebugStruct<'a, 'b: 'a>(fmt::DebugStruct<'a, 'b>);
impl<'a, 'b: 'a> SerializeStruct for DebugStruct<'a, 'b> {
    type Ok = (); type Error = Error;
    fn serialize_field<T>(&mut self, k: &'static str, v: &T) -> Result<(), Error> where T: ?Sized + Serialize {
        self.0.field(k, &to_debug(v)); Ok(())
    }
    fn end(mut self) -> Result<(), Error> { self.0.finish().map_err(|_| Error) }
}

struct DebugStructVariant<'a, 'b: 'a>(fmt::DebugStruct<'a, 'b>);
impl<'a, 'b: 'a> SerializeStructVariant for DebugStructVariant<'a, 'b> {
    type Ok = (); type Error = Error;
    fn serialize_field<T>(&mut self, k: &'static str, v: &T) -> Result<(), Error> where T: ?Sized + Serialize {
        self.0.field(k, &to_debug(v)); Ok(())
    }
    fn end(mut self) -> Result<(), Error> { self.0.finish().map_err(|_| Error) }
}

struct DebugMap<'a, 'b: 'a>(fmt::DebugMap<'a, 'b>);
impl<'a, 'b: 'a> SerializeMap for DebugMap<'a, 'b> {
    type Ok = (); type Error = Error;
    fn serialize_entry<K, V>(&mut self, k: &K, v: &V) -> Result<(), Error>
    where K: ?Sized + Serialize, V: ?Sized + Serialize {
        self.0.entry(&to_debug(k), &to_debug(v)); Ok(())
    }
    fn serialize_key<T>(&mut self, k: &T) -> Result<(), Error> where T: ?Sized + Serialize {
        self.0.key(&to_debug(k)); Ok(())
    }
    fn serialize_value<T>(&mut self, v: &T) -> Result<(), Error> where T: ?Sized + Serialize {
        self.0.value(&to_debug(v)); Ok(())
    }
    fn end(mut self) -> Result<(), Error> { self.0.finish().map_err(|_| Error) }
}

#[derive(Debug)]
pub struct Error;

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "failed to serialize to a standard formatter")
    }
}

// NOTE: The original serde_fmt has `impl From<Error> for fmt::Error` here.
// We deliberately omit it to prevent ambiguity with stylo's ToCss derive macro,
// which relies on the unique `From<T> for T` identity impl for `fmt::Error`.

impl From<fmt::Error> for Error {
    fn from(_: fmt::Error) -> Error {
        Error
    }
}

impl ser::StdError for Error {}

impl ser::Error for Error {
    fn custom<T>(_: T) -> Self
    where T: Display {
        Error
    }
}
