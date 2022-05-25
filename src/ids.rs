#![allow(dead_code)]

use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Copy, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct NumId<T> {
    value: i64,
    #[serde(skip)]
    _mark: PhantomData<T>
}

impl<T> NumId<T> {
    pub fn new(id: i64) -> NumId<T> {
        Self { value: id, _mark: PhantomData }
    }

    pub fn as_ref(&self) -> &i64 {
        &self.value
    }

    pub fn breach_extract(&self) -> i64 {
        self.value
    }
}

impl<T> Display for NumId<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.value, f)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct StringId<T> {
    value: String,
    #[serde(skip)]
    _mark: PhantomData<T>
}

impl<T> StringId<T> {
    pub fn new(id: impl Into<String>) -> StringId<T> {
        Self { value: id.into(), _mark: PhantomData }
    }

    pub fn as_ref(&self) -> &str {
        &self.value
    }

    pub fn breach_inner(self) -> String {
        self.value
    }
}

impl<T> Display for StringId<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.value, f)
    }
}

impl<T> Default for StringId<T> {
    fn default() -> Self {
        StringId::new(String::new())
    }
}

impl<T> From<StringId<T>> for String {
    fn from(id: StringId<T>) -> Self {
        id.value
    }
}

#[cfg(test)]
mod id_test {
    use serde::{Deserialize, Serialize};
    use crate::ids::NumId;

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    struct Entry {
        id: NumId<Entry>
    }

    #[test]
    fn parse_test() {
        let e = Entry {
            id: NumId::new(1)
        };

        let s = serde_json::to_string_pretty(&e).expect("");
        println!("{}", s);
        let d: Entry = serde_json::from_str(&s).expect("");
        assert_eq!(e, d);
    }
}