fn main() {
    let program_id: solana_sdk::pubkey::Pubkey = "FGJS4S51o9rSvxeomGrqacdwPFnZbBuU6p9KzhRHUx3b".parse().unwrap();
    let (pda, bump) = solana_sdk::pubkey::Pubkey::find_program_address(&[b"vault_config"], &program_id);
    println!("vault_config PDA: {}", pda);
    println!("bump: {}", bump);
}
