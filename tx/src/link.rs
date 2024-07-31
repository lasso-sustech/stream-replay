
use serde::de::Deserializer;
use serde::{Serialize, Deserialize};
#[derive(Serialize, Debug,Clone)]
pub struct Link {
    pub tx_ipaddr: String,
    pub rx_ipaddr: String,
}

impl<'de> Deserialize<'de> for Link {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let arr: [String; 2] = Deserialize::deserialize(deserializer)?;
        Ok(Link {
            tx_ipaddr: arr[0].clone(),
            rx_ipaddr: arr[1].clone(),
        })
    }
}