//! Example crate for testing Bronzite compile-time reflection.
//!
//! This crate defines some types and traits that can be introspected
//! using Bronzite's daemon and macros.

/// A simple trait for demonstration.
pub trait MyTrait {
    fn do_something(&self) -> String;
}

/// A struct that implements MyTrait.
pub struct Foo {
    pub value: i32,
}

impl MyTrait for Foo {
    fn do_something(&self) -> String {
        format!("Foo does something with value: {}", self.value)
    }
}

/// A struct with inherent methods.
pub struct Bar {
    name: String,
}

impl Bar {
    pub fn new(name: String) -> Self {
        Self { name }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
}

/// A generic struct.
pub struct Baz<T> {
    pub data: T,
}

impl<T> Baz<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }

    pub fn into_inner(self) -> T {
        self.data
    }
}

/// Another trait with an associated type.
pub trait AnotherTrait {
    type Output;

    fn transform(&self) -> Self::Output;
}

impl AnotherTrait for i32 {
    type Output = String;

    fn transform(&self) -> Self::Output {
        format!("Number: {}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_foo() {
        let foo = Foo { value: 42 };
        assert_eq!(foo.do_something(), "Foo does something with value: 42");
    }

    #[test]
    fn test_bar() {
        let bar = Bar::new("test".to_string());
        assert_eq!(bar.get_name(), "test");
    }

    #[test]
    fn test_baz() {
        let baz = Baz::new(vec![1, 2, 3]);
        assert_eq!(baz.data, vec![1, 2, 3]);
    }

    #[test]
    fn test_another_trait() {
        let num = 123;
        assert_eq!(num.transform(), "Number: 123");
    }
}
