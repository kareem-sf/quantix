// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    if let [_, flag, mode, challenge, application_home, resource_directory] = arguments.as_slice() {
        if flag == "--quantix-acceptance-rehearsal" {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| ())
                .and_then(|runtime| {
                    runtime
                        .block_on(quantix_lib::print_candidate_acceptance_rehearsal(
                            challenge,
                            mode,
                            application_home,
                            resource_directory,
                        ))
                        .map_err(|_| ())
                });
            if result.is_err() {
                std::process::exit(2);
            }
            return;
        }
    }
    if let [_, flag, challenge] = arguments.as_slice() {
        if flag == "--quantix-acceptance-probe" {
            if quantix_lib::print_candidate_acceptance_probe(challenge).is_err() {
                std::process::exit(2);
            }
            return;
        }
    }
    if quantix_lib::run_update_rollback_helper_from_args() {
        return;
    }
    quantix_lib::run()
}
