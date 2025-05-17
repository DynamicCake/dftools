pub mod auth;
pub mod baton;
pub mod instance;

// They cannot be negative, it is just because postgres can return negatives
pub type PlotId = i32;
