
pub mod baton;
pub mod instance;
pub mod auth;

// They cannot be negative, it is just because postgres can return negatives
pub type PlotId = i32;

