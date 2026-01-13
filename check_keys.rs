use rocksdb::DB;

fn main() {
    let db = DB::open_default("./blockchain_data").unwrap();
    println!("All keys in database:");
    let iter = db.iterator(rocksdb::IteratorMode::Start);
    for (key, _) in iter {
        let key_str = String::from_utf8_lossy(&key);
        println!("  {}", key_str);
    }
}
