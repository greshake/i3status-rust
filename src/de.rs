use serde::de::{self, Deserialize, Deserializer, DeserializeSeed};
use std::collections::HashMap as Map;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;

pub struct MapType<T, K, V>(pub PhantomData<T>, pub PhantomData<K>, pub PhantomData<V>);

macro_rules! map_type {
    ( $name:ident, $key:ty, $value:ty; $fromstr_ident:ident => $fromstr_expr:expr ) => {
        #[derive(Deserialize, Debug, Default)]
        struct $name(::std::collections::HashMap<$key, $value>);

        impl Deref for $name {
            type Target = ::std::collections::HashMap<$key, $value>;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl From<::std::collections::HashMap<$key, $value>> for $name {
            fn from(m: ::std::collections::HashMap<$key, $value>) -> Self {
                $name(m)
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str($fromstr_ident: &str) -> Result<Self, Self::Err> {
                $fromstr_expr
            }
        }
    }
}

impl<'de, T, K, V> DeserializeSeed<'de> for MapType<T, K, V>
where
    T: Deserialize<'de> + Default + FromStr<Err = String> + From<Map<K, V>> + Deref<Target=Map<K, V>>,
    K: Deserialize<'de> + Eq + ::std::hash::Hash + Clone,
    V: Deserialize<'de> + Clone,
{
    type Value = Map<K, V>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>
    {
        deserializer.deserialize_any(self)
    }
}

impl<'de, T, K, V> de::Visitor<'de> for MapType<T, K, V>
where
    T: Deserialize<'de> + Default + FromStr<Err = String> + From<Map<K, V>> + Deref<Target=Map<K, V>>,
    K: Deserialize<'de> + Eq + ::std::hash::Hash + Clone,
    V: Deserialize<'de> + Clone,
{
    type Value = Map<K, V>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string, seq or map")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error
    {
        let t: T = FromStr::from_str(value).map_err(de::Error::custom)?;
        Ok(t.clone())
    }

    fn visit_seq<A>(self, mut visitor: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>
    {
        let mut vec: Vec<Self::Value> = Vec::new();
        while let Some(element) = visitor.next_element_seed(MapType::<T, K, V>(PhantomData, PhantomData, PhantomData))? {
            vec.push(element);
        }

        if vec.is_empty() {
            Err(de::Error::custom("seq is empty"))
        } else {
            let mut combined = vec.remove(0).clone();
            for other in vec {
                combined.extend(other);
            }
            Ok(combined)
        }
    }

    fn visit_map<A>(self, visitor: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>
    {
        Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))
    }
}
