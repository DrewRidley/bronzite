//! Example types crate for demonstrating Bronzite reflection.
//!
//! This crate defines various types and traits that can be introspected
//! at compile time using Bronzite.

/// A trait for serializable entities.
pub trait Serialize {
    fn serialize(&self) -> String;
}

/// A trait for entities with an ID.
pub trait HasId {
    type Id;
    fn id(&self) -> Self::Id;
}

/// A user entity.
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub active: bool,
}

impl User {
    pub fn new(id: u64, name: String, email: String) -> Self {
        Self {
            id,
            name,
            email,
            active: true,
        }
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Serialize for User {
    fn serialize(&self) -> String {
        format!(
            r#"{{"id":{},"name":"{}","email":"{}","active":{}}}"#,
            self.id, self.name, self.email, self.active
        )
    }
}

impl HasId for User {
    type Id = u64;

    fn id(&self) -> Self::Id {
        self.id
    }
}

/// A product entity.
#[derive(Debug, Clone)]
pub struct Product {
    pub sku: String,
    pub name: String,
    pub price: f64,
}

impl Product {
    pub fn new(sku: String, name: String, price: f64) -> Self {
        Self { sku, name, price }
    }

    pub fn apply_discount(&mut self, percent: f64) {
        self.price *= 1.0 - (percent / 100.0);
    }
}

impl Serialize for Product {
    fn serialize(&self) -> String {
        format!(
            r#"{{"sku":"{}","name":"{}","price":{}}}"#,
            self.sku, self.name, self.price
        )
    }
}

impl HasId for Product {
    type Id = String;

    fn id(&self) -> Self::Id {
        self.sku.clone()
    }
}

/// An enum representing order status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Processing,
    Shipped,
    Delivered,
    Cancelled,
}

impl Serialize for OrderStatus {
    fn serialize(&self) -> String {
        match self {
            OrderStatus::Pending => r#""pending""#.to_string(),
            OrderStatus::Processing => r#""processing""#.to_string(),
            OrderStatus::Shipped => r#""shipped""#.to_string(),
            OrderStatus::Delivered => r#""delivered""#.to_string(),
            OrderStatus::Cancelled => r#""cancelled""#.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_serialize() {
        let user = User::new(1, "Alice".to_string(), "alice@example.com".to_string());
        assert!(user.serialize().contains("Alice"));
    }

    #[test]
    fn test_product_discount() {
        let mut product = Product::new("SKU001".to_string(), "Widget".to_string(), 100.0);
        product.apply_discount(10.0);
        assert!((product.price - 90.0).abs() < 0.01);
    }
}
