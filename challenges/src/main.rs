use std::env;

mod challenge01;
mod challenge02;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let challenge = &args[1];

    match challenge.as_str() {
        "1" => challenge01::run().unwrap(),
        "2" => challenge02::run().await?,
        _ => panic!(),
    }

    Ok(())
}