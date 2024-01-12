use std::env;

mod pipeliner;
mod rayon;
mod rust_ssp;
mod sequential;
mod spar_rust;
mod spar_rust_v2;
mod std_threads;
mod tokio;

pub const BLOCK_SIZE: usize = 900000;
fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        return Err(format!(
            "Correct usage: $ {} <runtime> <nthreads> <compress/decompress> <file name>",
            args[0]
        ));
    }
    let run_mode = &args[1];
    let threads = args[2]
        .parse::<usize>()
        .expect("nthreads argument must be a positive number");
    let file_action = &args[3];
    let file_name = &args[4];

    match run_mode.as_str() {
        "sequential" => sequential::sequential(file_action, file_name),
        "sequential-io" => sequential::sequential_io(file_action, file_name),
        "rust-ssp" => rust_ssp::rust_ssp(threads, file_action, file_name),
        "rust-ssp-io" => rust_ssp::rust_ssp_io(threads, file_action, file_name),
        "spar-rust" => spar_rust::spar_rust(threads, file_action, file_name),
        "spar-rust-io" => spar_rust::spar_rust_io(threads, file_action, file_name),
        "spar-rust-v2" => spar_rust_v2::spar_rust_v2(threads, file_action, file_name),
        "spar-rust-v2-io" => spar_rust_v2::spar_rust_v2_io(threads, file_action, file_name),
        "std-threads" => std_threads::std_threads(threads, file_action, file_name),
        "std-threads-io" => std_threads::std_threads_io(threads, file_action, file_name),
        "tokio" => tokio::tokio(threads, file_action, file_name),
        "tokio-io" => tokio::tokio_io(threads, file_action, file_name),
        "rayon" => rayon::rayon(threads, file_action, file_name),
        "pipeliner" => pipeliner::pipeliner(threads, file_action, file_name),
        _ => println!("Invalid run_mode '{run_mode}', use: sequential | rust-ssp | spar-rust | spar-rust-io | std-threads | std-threads-io | tokio | tokio-io | rayon | pipeliner"),
    }

    Ok(())
}
