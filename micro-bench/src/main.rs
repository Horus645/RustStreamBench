mod pipeliner;
mod rayon;
mod rust_ssp;
mod sequential;
mod spar_rust;
mod spar_rust_mpi;
mod spar_rust_v2;
mod std_threads;
mod tokio;
mod mpi;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 6 {
        panic!(
            "Correct usage: $ ./{:?} <runtime> <img size> <nthreads> <iter size 1> <iter size 2>",
            args[0]
        );
    }
    let runtime = &args[1];
    let size = args[2].parse::<usize>().unwrap();
    let threads = args[3].parse::<usize>().unwrap();
    let iter_size1 = args[4].parse::<i32>().unwrap();
    let iter_size2 = args[5].parse::<i32>().unwrap();

    assert!(threads > 0);
    match runtime.as_str() {
        "sequential" => sequential::sequential(size, iter_size1, iter_size2),
        #[cfg(feature = "multithreaded")]
        "rust-ssp" => rust_ssp::rust_ssp_pipeline(size, threads, iter_size1, iter_size2),
        #[cfg(feature = "multithreaded")]
        "spar-rust" => spar_rust::spar_rust_pipeline(size, threads, iter_size1, iter_size2),
        #[cfg(feature = "multithreaded")]
        "spar-rust-v2" => spar_rust_v2::spar_rust_v2_pipeline(size, threads, iter_size1, iter_size2),
        #[cfg(feature = "multithreaded")]
        "std-threads" => std_threads::std_threads_pipeline(size, threads, iter_size1, iter_size2),
        #[cfg(feature = "multithreaded")]
        "tokio" => tokio::tokio_pipeline(size, threads, iter_size1, iter_size2),
        #[cfg(feature = "multithreaded")]
        "rayon" => rayon::rayon_pipeline(size, threads, iter_size1, iter_size2),
        #[cfg(feature = "multithreaded")]
        "pipeliner" => pipeliner::pipeliner_pipeline(size, threads, iter_size1, iter_size2),
        #[cfg(feature = "mpi")]
        "mpi" => mpi::rsmpi_pipeline(size, threads, iter_size1, iter_size2),
        #[cfg(feature = "mpi")]
        "spar-rust-mpi" => spar_rust_mpi::spar_rust_mpi_pipeline(size, threads, iter_size1, iter_size2),
        _ => println!("Invalid run_mode, use: sequential | rust-ssp | spar-rust | std-threads | tokio | rayon | pipeliner"),
    }
}
