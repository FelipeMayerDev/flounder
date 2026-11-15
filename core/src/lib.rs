//! Núcleo compartilhado: estado e cálculos (`model`), contrato JSON com o backend (`api`)
//! e cartões conectados (`connected`). Nada aqui conhece egui nem tokio — desktop, mobile
//! e backend usam estes três módulos por caminhos iguais (`flounder::model`, `crate::api`…).
pub mod api;
pub mod connected;
pub mod model;
