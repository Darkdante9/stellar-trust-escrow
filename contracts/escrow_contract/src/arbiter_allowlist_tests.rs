#[cfg(test)]
#[allow(clippy::module_inception)]
mod arbiter_allowlist_tests {
    use soroban_sdk::{testutils::Address as _, Address, Env};

    use crate::{EscrowContract, EscrowContractClient, EscrowError};

    fn setup() -> (Env, Address, EscrowContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);
        client.initialize(&admin);

        (env, admin, client)
    }

    #[test]
    fn test_add_to_allowlist_requires_admin() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);
        let non_admin = Address::generate(&env);

        let result = contract.try_add_to_arbiter_allowlist(&non_admin, &arbiter);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_to_allowlist_happy_path() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);

        contract.add_to_arbiter_allowlist(&admin, &arbiter);
        assert!(contract.is_arbiter_allowed(&arbiter));
    }

    #[test]
    fn test_add_to_allowlist_prevents_duplicates() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);

        contract.add_to_arbiter_allowlist(&admin, &arbiter);
        let result = contract.try_add_to_arbiter_allowlist(&admin, &arbiter);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_from_allowlist_requires_admin() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);
        let non_admin = Address::generate(&env);

        contract.add_to_arbiter_allowlist(&admin, &arbiter);
        let result = contract.try_remove_from_arbiter_allowlist(&non_admin, &arbiter);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_from_allowlist_happy_path() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);

        contract.add_to_arbiter_allowlist(&admin, &arbiter);
        contract.remove_from_arbiter_allowlist(&admin, &arbiter);
        assert!(!contract.is_arbiter_allowed(&arbiter));
    }

    #[test]
    fn test_remove_from_allowlist_not_found() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);

        let result = contract.try_remove_from_arbiter_allowlist(&admin, &arbiter);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_arbiter_allowed_default_false() {
        let (env, _admin, contract) = setup();
        let arbiter = Address::generate(&env);

        assert!(!contract.is_arbiter_allowed(&arbiter));
    }

    // ---------------------------------------------------------------------
    // Invariant tests for the arbiter selection lifecycle
    // ---------------------------------------------------------------------

    #[test]
    fn test_lifecycle_success_path_allowlisted_arbiter_resolves_dispute() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);
        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);

        // Admin allowlists the arbiter.
        contract.add_to_arbiter_allowlist(&admin, &arbiter);
        assert!(contract.is_arbiter_allowed(&arbiter));

        // Escrow is created with the allowlisted arbiter.
        let escrow_id = contract.create_escrow(&buyer, &seller, &arbiter, &1000_i128);
        assert_eq!(contract.get_escrow(&escrow_id).arbiter, arbiter);

        // The allowlisted arbiter can resolve the dispute.
        contract.resolve_dispute(&arbiter, &escrow_id, &true);
        assert!(contract.get_escrow(&escrow_id).resolved);
    }

    #[test]
    fn test_lifecycle_rejection_non_admin_add_and_remove() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);
        let non_admin = Address::generate(&env);

        // Non-admin add is rejected with E87.
        let add_result = contract.try_add_to_arbiter_allowlist(&non_admin, &arbiter);
        assert_eq!(add_result, Err(Ok(EscrowError::Unauthorized)));
        assert!(!contract.is_arbiter_allowed(&arbiter));

        // Non-admin remove is rejected with E87 and leaves state untouched.
        contract.add_to_arbiter_allowlist(&admin, &arbiter);
        let remove_result = contract.try_remove_from_arbiter_allowlist(&non_admin, &arbiter);
        assert_eq!(remove_result, Err(Ok(EscrowError::Unauthorized)));
        assert!(contract.is_arbiter_allowed(&arbiter));
    }

    #[test]
    fn test_lifecycle_rejection_duplicate_add() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);

        contract.add_to_arbiter_allowlist(&admin, &arbiter);
        let result = contract.try_add_to_arbiter_allowlist(&admin, &arbiter);
        assert_eq!(result, Err(Ok(EscrowError::ArbiterAlreadyAllowed)));
        assert!(contract.is_arbiter_allowed(&arbiter));
    }

    #[test]
    fn test_lifecycle_rejection_remove_missing_arbiter() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);

        let result = contract.try_remove_from_arbiter_allowlist(&admin, &arbiter);
        assert_eq!(result, Err(Ok(EscrowError::ArbiterNotAllowed)));
        assert!(!contract.is_arbiter_allowed(&arbiter));
    }

    #[test]
    fn test_lifecycle_rejection_non_allowlisted_arbiter_assignment() {
        let (env, _admin, contract) = setup();
        let arbiter = Address::generate(&env);
        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);

        // Arbiter is not on the allowlist, so escrow creation is rejected.
        let result = contract.try_create_escrow(&buyer, &seller, &arbiter, &1000_i128);
        assert_eq!(result, Err(Ok(EscrowError::ArbiterNotAllowed)));
        assert!(!contract.is_arbiter_allowed(&arbiter));
    }

    #[test]
    fn test_lifecycle_boundary_empty_allowlist() {
        let (env, _admin, contract) = setup();
        let arbiter = Address::generate(&env);

        // With an empty allowlist no arbiter is allowed.
        assert!(!contract.is_arbiter_allowed(&arbiter));
    }

    #[test]
    fn test_lifecycle_boundary_add_remove_readd() {
        let (env, admin, contract) = setup();
        let arbiter = Address::generate(&env);

        // Add -> allowed.
        contract.add_to_arbiter_allowlist(&admin, &arbiter);
        assert!(contract.is_arbiter_allowed(&arbiter));

        // Remove -> not allowed.
        contract.remove_from_arbiter_allowlist(&admin, &arbiter);
        assert!(!contract.is_arbiter_allowed(&arbiter));

        // Re-add -> allowed again.
        contract.add_to_arbiter_allowlist(&admin, &arbiter);
        assert!(contract.is_arbiter_allowed(&arbiter));
    }

    #[test]
    fn test_lifecycle_boundary_multiple_arbiters_independent() {
        let (env, admin, contract) = setup();
        let arbiter_a = Address::generate(&env);
        let arbiter_b = Address::generate(&env);

        contract.add_to_arbiter_allowlist(&admin, &arbiter_a);
        contract.add_to_arbiter_allowlist(&admin, &arbiter_b);
        assert!(contract.is_arbiter_allowed(&arbiter_a));
        assert!(contract.is_arbiter_allowed(&arbiter_b));

        // Removing one arbiter does not affect the other.
        contract.remove_from_arbiter_allowlist(&admin, &arbiter_a);
        assert!(!contract.is_arbiter_allowed(&arbiter_a));
        assert!(contract.is_arbiter_allowed(&arbiter_b));
    }
}
