use std::env;

mod mpi;
mod pipeliner;
mod rayon;
mod rust_ssp;
mod sequential;
mod spar_rust;
mod spar_rust_mpi;
mod spar_rust_v2;
mod std_threads;
mod tokio;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!();
        panic!(
            "Correct usage: $ ./{:?} <runtime> <nthreads> <images dir>",
            args[0]
        );
    }
    let run_mode = &args[1];
    let threads = args[2].parse::<usize>().unwrap();
    let dir_name = &args[3];

    match run_mode.as_str() {
        "sequential" => sequential::sequential(dir_name),
        #[cfg(feature = "multithreaded")]
        "rust-ssp" => rust_ssp::rust_ssp(dir_name, threads),
        #[cfg(feature = "multithreaded")]
        "spar-rust" => spar_rust::spar_rust(dir_name, threads),
        #[cfg(feature = "multithreaded")]
        "spar-rust-v2" => spar_rust_v2::spar_rust_v2(dir_name, threads),
        #[cfg(feature = "multithreaded")]
        "pipeliner" => pipeliner::pipeliner(dir_name, threads),
        #[cfg(feature = "multithreaded")]
        "tokio" => tokio::tokio(dir_name, threads),
        #[cfg(feature = "multithreaded")]
        "rayon" => rayon::rayon(dir_name, threads),
        #[cfg(feature = "multithreaded")]
        "std-threads" => std_threads::std_threads(dir_name, threads),

        #[cfg(feature = "mpi")]
        "mpi" => mpi::rsmpi(dir_name, threads),
        #[cfg(feature = "mpi")]
        "spar-rust-mpi" => spar_rust_mpi::spar_rust_mpi(dir_name, threads),
        _ => println!("Invalid run_mode, use: sequential | rust-ssp | spar-rust | std-threads | tokio | rayon | pipeliner"),
    }
    Ok(())
}
