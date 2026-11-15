pub mod app;
mod dashboard;
pub mod screens;
pub mod widgets;
pub mod backend;
pub mod assistant;
pub mod update;

// Estado, cálculos e contrato JSON vivem no núcleo compartilhado; os módulos são
// reexportados para os caminhos antigos (`crate::model`, `flounder::api`…) seguirem iguais.
pub use flounder_core::{api, connected, model};
