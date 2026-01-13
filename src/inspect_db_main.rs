//! Database inspection binary

use atomiq::db_inspector::inspect_database;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "./blockchain_data";
    inspect_database(db_path)?;
    Ok(())
}