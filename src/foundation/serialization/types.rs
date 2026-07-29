#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object(pub(super) Vec<(String, Value)>);

impl Object {
    pub(super) fn new() -> Self {
        Self(Vec::new())
    }

    pub(super) fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        if let Some((_, stored)) = self.0.iter_mut().find(|(stored, _)| stored == &key) {
            return Some(std::mem::replace(stored, value));
        }
        self.0.push((key, value));
        None
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0
            .iter()
            .find_map(|(stored, value)| (stored == key).then_some(value))
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.iter().map(|(key, _)| key)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.iter().any(|(stored, _)| stored == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalObject {
    pub entries: Vec<(String, CanonicalValue)>,
}

impl CanonicalObject {
    pub fn get(&self, key: &str) -> Option<&CanonicalValue> {
        self.entries
            .iter()
            .find_map(|(stored, value)| (stored == key).then_some(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalValue {
    Object(CanonicalObject),
    Array(Vec<CanonicalValue>),
    String(String),
    Unsigned { raw: String },
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Object(Object),
    Array(Vec<Value>),
    String(String),
    Number(u128),
    Decimal(String),
    Bool(bool),
    Null,
}
