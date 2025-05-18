use std::collections::HashMap;

use poem_openapi::{Object, Union};
use redis_macros::{FromRedisValue, ToRedisArgs};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Union, ToRedisArgs, FromRedisValue)]
#[oai(discriminator_name = "id", rename_all = "snake_case")]
#[serde(tag = "id")]
#[serde(rename_all = "snake_case")]
pub enum DfJson {
    Dict(DfDict),
    Comp(DfComp),
    Str(DfString),
    Num(DfNumber),
    Loc(DfLoc),
    Vec(DfVec),
    Sound(DfSound),
    Particle(DfParticle),
    Potion(DfPotion),
    List(DfList),
    /*
     * TODO: Add item data type
     */
}
#[derive(Serialize, Deserialize, JsonSchema, Object)]
pub struct DfList {
    val: Vec<DfJson>,
}
#[derive(Serialize, Deserialize, JsonSchema, Object)]
pub struct DfNumber {
    val: f64,
}
#[derive(Serialize, Deserialize, JsonSchema, Object)]
pub struct DfString {
    val: String,
}
#[derive(Serialize, Deserialize, JsonSchema, Object)]
pub struct DfComp {
    val: String,
}
#[derive(Serialize, Deserialize, JsonSchema, Object)]
pub struct DfDict {
    val: HashMap<String, DfJson>,
}
#[derive(Serialize, Deserialize, JsonSchema, Object)]
pub struct DfPotion {
    potion: String,
    duration: f64,
    amplifier: f64,
}

#[derive(Serialize, Deserialize, JsonSchema, Object)]
pub struct DfParticle {
    particle: String,
    cluster: ParticleCluster,
    data: ParticleData,
}
#[derive(Serialize, Deserialize, JsonSchema, Object)]
pub struct DfSound {
    sound: String,
    variant: String,
    pitch: f64,
    volume: f64,
}
#[derive(Serialize, Deserialize, JsonSchema, Object)]
pub struct DfVec {
    x: f64,
    y: f64,
    z: f64,
}
#[derive(Serialize, Deserialize, JsonSchema, Object)]
pub struct DfLoc {
    x: f64,
    y: f64,
    z: f64,
    pitch: f64,
    yaw: f64,
}

#[derive(JsonSchema, Serialize, Deserialize, Object)]
pub struct ParticleData {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    pub motion_variation: Option<f64>,
    pub size: Option<f64>,
    pub size_variation: Option<f64>,
    pub color: Option<String>, // Stored in hex
    pub color_variation: Option<f64>,
    pub color_fade: Option<String>,
    pub roll: Option<f64>,
    pub material: Option<String>,
    pub opacity: Option<f64>,
}

#[derive(JsonSchema, Serialize, Deserialize, Object)]
pub struct ParticleCluster {
    pub horizontal: f64,
    pub vertical: f64,
    pub amount: f64,
}

/*
{
    "type": "dict",
    "players": {
        "type": "list",
        "val": [
            {
                "type": "str",
                "val": "Notch"
            },
            {
                "type": "str",
                "val": "Jeremaster"
            }
        ]
    }
}
*/
