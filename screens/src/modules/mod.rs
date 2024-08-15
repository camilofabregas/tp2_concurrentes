pub mod parser;
pub mod utils;
// Por alguna razón, si no lo pongo en mayúscula, rust analyzer no lo detecta y no funcionan las
// sugerencias en screen.rs, aunque el programa funciona bien
// #[allow(non_snake_case)]
// pub mod Screen;
pub mod connections;
pub mod init;
pub mod screen;
