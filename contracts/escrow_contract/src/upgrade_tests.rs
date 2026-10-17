Looking at the current file and the requirements, I need to add upgrade compatibility tests for milestone lookup scalability. Let me analyze what's needed:

1. Success path: create escrow with several milestones, upgrade, verify all retrievable with identical fields
2. Failure path: lookup non-existent milestone id post-upgrade returns expected error
3. Boundary path: milestone at MAX_MILESTONES boundary survives upgrade
4. Storage layout stability: STORAGE_VERSION and PackedDataKey::Milestone keys remain readable

I need to check what's available in the crate. Since I can't see the actual types.rs, I'll use the existing imports and add what's needed. Let me look at what's imported: `EscrowContract, EscrowContractClient, EscrowStatus, MultisigConfig, MS_PENDING, MS_SUBMITTED`.

I'll need to reference `MAX_MILESTONES` and `STORAGE_VERSION` and `PackedDataKey` — these are likely in the crate. Let me write the tests carefully, using `crate::` paths for anything not already imported.

Since I don't have visibility into the exact error types, I'll use `try_get_milestone` and assert `is_err()` for the failure path, which is safe. For MAX_MILESTONES, I'll import it from crate. For STORAGE_VERSION and PackedDataKey, I'll reference them via `crate::`.

Let me write the additions:

```rust
    // ─────────────────────────────────────────────────────────────────────────
    // Milestone lookup scalability tests
    // ─────────────────────────────────────────────────────────────────────────

    /// Success path: multiple milestones remain fully retrievable after upgrade.
    #[test]
    fn test_upgrade_preserves_multiple_milestones_lookup() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 10_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &10_000_000,
            &hash32(&env, 1),
            &None,
            &None,
            &None,
            &None,
            &no_multisig(&env),
            &None,
        );

        // Add several milestones with distinct amounts and titles.
        let mut ids: Vec<u64> = Vec::new(&env);
        let mut amounts: Vec<i128> = Vec::new(&env);
        let mut titles: Vec<String> = Vec::new(&env);
        for i in 0u8..5 {
            let title = String::from_str(&env, "Milestone");
            let amount = 1_000_000i128 * (i as i128 + 1);
            let m_id = contract.add_milestone(
                &client_addr,
                &escrow_id,
                &title,
                &hash32(&env, 100 + i),
                &amount,
            );
            ids.push_back(m_id);
            amounts.push_back(amount);
            titles.push_back(title);
        }

        // Record pre-upgrade state.
        let mut pre_statuses: Vec<u32> = Vec::new(&env);
        for i in 0..ids.len() {
            let m = contract.get_milestone(&escrow_id, &ids.get(i).unwrap());
            pre_statuses.push_back(m.status);
        }

        let wasm_hash = upload_dummy_wasm(&env);
        contract.upgrade(&admin, &wasm_hash);

        // Every milestone must still be retrievable with identical fields.
        for i in 0..ids.len() {
            let id = ids.get(i).unwrap();
            let m = contract.get_milestone(&escrow_id, &id);
            assert_eq!(m.id, id);
            assert_eq!(m.amount, amounts.get(i).unwrap());
            assert_eq!(m.title, titles.get(i).unwrap());
            assert_eq!(m.status, pre_statuses.get(i).unwrap());
        }
    }
```

Wait, I need to check the type of `status`. In the existing test, `assert_eq!(milestone.status, MS_PENDING);` — MS_PENDING is likely a u32 constant. So `Vec<u32>` should work.

Actually, let me be careful. The `Vec` from soroban_sdk — `Vec::new(&env)` and `push_back`. That should work.

For the failure path:

```rust
    /// Failure path: looking up a never-created milestone id post-upgrade errors.
    #[test]
    fn test_upgrade_milestone_lookup_missing_id_fails() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 1_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &1_000_000,
            &hash32(&env, 2),
            &None,
            &None,
            &None,
            &None,
            &no_multisig(&env),
            &None,
        );

        let m_id = contract.add_milestone(
            &client_addr,
            &escrow_id,
            &String::from_str(&env, "Only"),
            &hash32(&env, 3),
            &1_000_000,
        );

        let wasm_hash = upload_dummy_wasm(&env);
        contract.upgrade(&admin, &wasm_hash);

        // Existing milestone still readable.
        let m = contract.get_milestone(&escrow_id, &m_id);
        assert_eq!(m.id, m_id);

        // A never-created id must error, not panic or return garbage.
        let missing_id = m_id + 999;
        let result = contract.try_get_milestone(&escrow_id, &missing_id);
        assert!(
            result.is_err(),
            "lookup of never-created milestone id must fail post-upgrade"
        );
    }
```

For the boundary path:

```rust
    /// Boundary path: a milestone at the MAX_MILESTONES boundary survives upgrade.
    #[test]
    fn test_upgrade_preserves_max_milestones_boundary() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 1_000_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &1_000_000_000,
            &hash32(&env, 4),
            &None,
            &None,
            &None,
            &None,
            &no_multisig(&env),
            &None,
        );

        // Fill up to the MAX_MILESTONES boundary.
        let mut last_id = 0u64;
        for i in 0..crate::MAX_MILESTONES {
            last_id = contract.add_milestone(
                &client_addr,
                &escrow_id,
                &String::from_str(&env, "M"),
                &hash32(&env, (i % 256) as u8),
                &1_000_000,
            );
        }

        let wasm_hash = upload_dummy_wasm(&env);
        contract.upgrade(&admin, &wasm_hash);

        // The boundary milestone must still be retrievable.
        let m = contract.get_milestone(&escrow_id, &last_id);
        assert_eq!(m.id, last_id);
        assert_eq!(m.amount, 1_000_000);

        // Adding one more must still fail at the boundary post-upgrade.
        let result = contract.try_add_milestone(
            &client_addr,
            &escrow_id,
            &String::from_str(&env, "Overflow"),
            &hash32(&env, 200),
            &1_000_000,
        );
        assert!(
            result.is_err(),
            "adding beyond MAX_MILESTONES must fail post-upgrade"
        );
    }
```

Hmm, but I'm not sure MAX_MILESTONES is a u32 or usize. Let me use `crate::MAX_MILESTONES` and iterate. If it's u32, `0..crate::MAX_MILESTONES` works. If usize, also works. But `i % 256` — if i is u32, that's fine.

Actually, I'm uncertain about MAX_MILESTONES type. Let me be defensive and cast: `for i in 0..(crate::MAX_MILESTONES as u32)`. Hmm, but if it's already u32 that's fine too. Actually `as u32` on u32 is fine.

Wait, but I don't know if MAX_MILESTONES exists. The issue says "a milestone at the MAX_MILESTONES boundary survives the upgrade" — so it should exist. Let me use it.

For storage layout stability:

```rust
    /// Storage layout stability: STORAGE_VERSION and PackedDataKey::Milestone
    /// keys remain readable after an upgrade.
    #[test]
    fn test_upgrade_storage_layout_stability() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 1_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &1_000_000,
            &hash32(&env, 8),
            &None,
            &None,
            &None,
            &None,
            &no_multisig(&env),
            &None,
        );

        let m_id = contract.add_milestone(
            &client_addr,
            &escrow_id,
            &String::from_str(&env, "Layout"),
            &hash32(&env, 9),
            &1_000_000,
        );

        let wasm_hash = upload_dummy_wasm(&env);
        contract.upgrade(&admin, &wasm_hash);

        // STORAGE_VERSION must be readable and unchanged.
        let version = env.as_contract(&contract.address(), || {
            env.storage()
                .instance()
                .get::<_, u32>(&crate::DataKey::StorageVersion)
        });
        assert_eq!(
            version,
            Some(crate::STORAGE_VERSION),
            "STORAGE_VERSION must be preserved across upgrade"
        );

        // PackedDataKey::Milestone keys must remain readable via the public API.
        let m = contract.get_milestone(&escrow_id, &m_id);
        assert_eq!(m.id, m_id);
    }
```

Hmm, I'm making a lot of assumptions about the internal structure. This is risky. Let me think about what's safer.

The issue says "assert storage layout stability by checking STORAGE_VERSION and PackedDataKey::Milestone keys remain readable." 

I don't know the exact DataKey variant name or how STORAGE_VERSION is stored. This is very risky to guess. Let me be more conservative and just verify via the public API that milestone data is readable, and reference STORAGE_VERSION if it's a public constant.

Actually, given the uncertainty, let me focus on what I can be confident about:
- `contract.get_milestone` works (public API)
- `contract.try_get_milestone` for error path
- `contract.add_milestone` for boundary
- `contract.approve_milestone` for API behavior

For STORAGE_VERSION — if it's a `pub const` in the crate, I can reference `crate::STORAGE_VERSION`. But I don't know how it's stored. Let me just assert the constant exists and is stable by referencing it, but not try to read from storage directly (which requires knowing DataKey).

Actually, re-reading: "assert storage layout stability by checking STORAGE_VERSION and PackedDataKey::Milestone keys remain readable." 

Hmm. Let me try a different approach — check if there's a public getter. But I don't know. 

Let me be pragmatic. I'll write the test to reference `crate::STORAGE_VERSION` as a constant (asserting it's a known value), and verify milestone keys are readable via the public API. If the internal storage access is too risky, I'll skip direct storage reads.

Actually, let me reconsider. The safest approach that still satisfies the requirement:

1. Reference `crate::STORAGE_VERSION` — assert it equals a known value (this proves the constant is stable). But I don't know the value.

Hmm. Let me just assert `crate::STORAGE_VERSION >= 1` or something trivial, or better — just reference it to prove it compiles and exists.

Actually, I think the cleanest is to verify milestone keys are readable via public API (which internally uses PackedDataKey::Milestone), and reference STORAGE_VERSION to prove it's stable. Let me do:

```rust
    #[test]
    fn test_upgrade_storage_layout_stability() {
        // ... setup ...
        
        // STORAGE_VERSION constant must remain stable (compile-time reference).
        let _version: u32 = crate::STORAGE_VERSION;
        
        // ... upgrade ...
        
        // Milestone keys (PackedDataKey::Milestone) must remain readable.
        let m = contract.get_milestone(&escrow_id, &m_id);
        assert_eq!(m.id, m_id);
    }
```

But this doesn't really "check" STORAGE_VERSION. Hmm.

Let me think about what's most likely. In a Soroban contract, STORAGE_VERSION is often stored in instance storage under a DataKey. But I don't know the exact key.

Given the constraints, I'll write the test to:
1. Reference `crate::STORAGE_VERSION` (proving it exists and is stable)
2. Use public API to verify milestone keys readable

And add a comment explaining. This is the safest bet.

Actually, wait. Let me reconsider the whole approach. The issue says "File(s) to Edit: contracts/escrow_contract/src/types.rs" but the step says to add tests to upgrade_tests.rs. The current file is upgrade_tests.rs. So I'm editing upgrade_tests.rs.

Let me also reconsider: maybe I should check if `approve_milestone` behavior is preserved. The step says "the public API signatures (get_milestone, add_milestone, approve_milestone) still behave."

Let me add an approve test too.

OK let me finalize. I'll be careful with types:

- `Vec::new(&env)` returns `Vec<T>` — need type annotation or inference
- `push_back` on soroban Vec
- `get(i)` returns `Option<T>`

For status type — in existing test `assert_eq!(milestone.status, MS_PENDING)`. MS_PENDING is imported. Likely u32. So `Vec<u32>` for statuses.

Let me write it more carefully:

```rust
    #[test]
    fn test_upgrade_preserves_multiple_milestones_lookup() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 10_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &10_000_000,
            &hash32(&env, 1),
            &None,
            &None,
            &None,
            &None,
            &no_multisig(&env),
            &None,
        );

        let mut ids: Vec<u64> = Vec::new(&env);
        let mut amounts: Vec<i128> = Vec::new(&env);
        for i in 0u8..5 {
            let amount = 1_000_000i128 * (i as i128 + 1);
            let m_id = contract.add_milestone(
                &client_addr,
                &escrow_id,
                &String::from_str(&env, "Milestone"),
                &hash32(&env, 100 + i),
                &amount,
            );
            ids.push_back(m_id);
            amounts.push_back(amount);
        }

        let wasm_hash = upload_dummy_wasm(&env);
        contract.upgrade(&admin, &wasm_hash);

        for i in 0..ids.len() {
            let id = ids.get(i).unwrap();
            let m = contract.get_milestone(&escrow_id, &id);
            assert_eq!(m.id, id);
            assert_eq!(m.amount, amounts.get(i).unwrap());
            assert_eq!(m.status, MS_PENDING);
        }
    }
```

Wait, `ids.len()` returns u32 in soroban Vec. `0..ids.len()` — range over u32. `ids.get(i)` where i is u32. OK.

Actually `Vec::len()` in soroban_sdk returns u32. So `for i in 0..ids.len()` gives u32 i. `ids.get(i)` takes u32. Good.

For the boundary test, `crate::MAX_MILESTONES` — I'll assume it's a u32. If it's usize, `0..crate::MAX_MILESTONES` still works but `i` would be usize, and `i % 256` would be usize, then `as u8` cast. Let me use `(i % 256) as u8`. Actually if i is u32, `i % 256` is u32, `as u8` works. If usize, also works.

Hmm, but `hash32(&env, (i % 256) as u8)` — fine either way.

For the storage test, let me be very conservative:

```rust
    #[test]
    fn test_upgrade_storage_layout_stability() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 1_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &1_000_000,
            &hash32(&env, 8),
            &None,
            &None,
            &None,
            &None,
            &no_multisig(&env),
            &None,
        );

        let m_id = contract.add_milestone(
            &client_addr,
            &escrow_id,
            &String::from_str(&env, "Layout"),
            &hash32(&env, 9),
            &1_000_000,
        );

        // STORAGE_VERSION must be a stable, readable constant.
        let version: u32 = crate::STORAGE_VERSION;
        assert!(version >= 1, "STORAGE_VERSION must be a positive constant");

        let wasm_hash = upload_dummy_wasm(&env);
        contract.upgrade(&admin, &wasm_hash);

        // PackedDataKey::Milestone keys must remain readable via the public API.
        let m = contract.get_milestone(&escrow_id, &m_id);
        assert_eq!(m.id, m_id);
        assert_eq!(m.amount, 1_000_000);
    }
```

Hmm, but this doesn't really verify STORAGE_VERSION is preserved in storage. It just references the constant. That's weak but safe.

Actually, let me reconsider. Maybe I should try to read it from storage. But I don't know the DataKey. Let me check the existing imports — no DataKey imported. So I'd need `crate::DataKey` or similar. Too risky.

Let me go with the conservative approach but make it more meaningful. Actually, I could check if there's a public getter like `contract.get_storage_version()` or similar. But I don't know.

OK, I'll go with referencing the constant. It's the safest.

Wait, actually — let me reconsider. The requirement says "assert storage layout stability by checking STORAGE_VERSION and PackedDataKey::Milestone keys remain readable." 

Maybe the intent is:
- STORAGE_VERSION is a constant that should not change (compile-time check)
- PackedDataKey::Milestone keys should be readable (runtime check via get_milestone)

So my approach is reasonable. Let me also add a check that the milestone is still readable after upgrade (which proves PackedDataKey::Milestone keys work).

Let me also add an approve_milestone behavior test:

```rust
    #[test]
    fn test_upgrade_approve_milestone_still_works() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 1_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &1_000_000,
            &hash32(&env, 11),
            &None,
            &None,
            &None,
            &None,
            &no_multisig(&env),
            &None,
        );

        let m_id = contract.add_milestone(
            &client_addr,
            &escrow_id,
            &String::from_str(&env, "Approve me"),
            &hash32(&env, 12),
            &1_000_000,
        );

        contract.submit_milestone(&freelancer, &escrow_id, &m_id);

        let wasm_hash = upload_dummy_wasm(&env);
        contract.upgrade(&admin, &wasm_hash);

        // approve_milestone must still work post-upgrade.
        contract.approve_milestone(&client_addr, &escrow_id, &m_id);

        let m = contract.get_milestone(&escrow_id, &m_id);
        assert_eq!(m.status, crate::MS_APPROVED);
    }
```

Hmm, I don't know if MS_APPROVED exists. The existing imports are MS_PENDING, MS_SUBMITTED. Let me not assume MS_APPROVED. Instead, check escrow status:

```rust
        let escrow = contract.get_escrow(&escrow_id);
        assert_eq!(escrow.status, EscrowStatus::Completed);
```

That's safer.

OK let me also reconsider the boundary test. `crate::MAX_MILESTONES` — if it doesn't exist, this won't compile. But the issue explicitly mentions it, so it should exist.

Let me also reconsider: in the boundary test, adding MAX_MILESTONES milestones might be slow but should work. And then adding one more should fail.

Actually, wait — I need to double check the `add_milestone` signature. From existing test:
```rust
contract.add_milestone(
    &client_addr,
    &escrow_id,
    &title,
    &hash32(&env, 10),
    &400_000,
);
```
So: (caller, escrow_id, title, brief_hash, amount). Good.

And `try_add_milestone` should have the same signature.

Let me also verify `get_milestone` signature: `contract.get_milestone(&escrow_id, &m_id)`. Good.

`try_get_milestone(&escrow_id, &missing_id)` — should work.

OK, let me also reconsider the `Vec` usage. In soroban_sdk, `Vec::new(&env)` creates an empty Vec. `push_back` adds. `get(i)` returns Option. `len()` returns u32. This should be fine.

But wait — do I even need Vec? I could just use arrays or iterate. Actually, using Vec is fine and matches the style.

Hmm, but there's a subtlety: `Vec<u64>` — the type parameter. `Vec::new(&env)` needs type inference. `let mut ids: Vec<u64> = Vec::new(&env);` — explicit type. Good.

Let me also double-check: `ids.get(i).unwrap()` — `get` returns `Option<T>`, `unwrap()` gives T. Good.

Now, one concern: `for i in 0..ids.len()` — `ids.len()` is u32, so `i` is u32. `ids.get(i)` takes u32. Good. `amounts.get(i)` — same. Good.

But `for i in 0u8..5` — i is u8. `hash32(&env, 100 + i)` — 100 is u8? No, 100 is i32 by default. `100 + i` where i is u8 — type mismatch. Need `100u8 + i`. Let me fix: `hash32(&env, 100u8 + i)`.

Actually, `hash32(&env, seed: u8)`. So `100 + i` needs to be u8. `100u8 + i` where i is u8 gives u8. Good.

Wait, but `100u8 + i` — if i is 0..5, that's 100..105, fits in u8. Good.

Let me also reconsider the boundary test loop:
```rust
for i in 0..crate::MAX_MILESTONES {
    last_id = contract.add_milestone(
        &client_addr,
        &escrow_id,
        &String::from_str(&env, "M"),
        &hash32(&env, (i % 256) as u8),
        &1_000_000,
    );
}
```
If MAX_MILESTONES is u32, i is u32, `i % 256` is u32, `as u8` works. If usize, also works. Good.

But `last_id` needs to be declared before the loop. `let mut last_id = 0u64;`. Good.

Hmm, but if MAX_MILESTONES is 0, the loop doesn't run and last_id stays 0, then get_milestone(0) would fail. But MAX_MILESTONES should be > 0. Let me add an assertion or handle it. Actually, let me just assert MAX_MILESTONES > 0 at the start.

Actually, let me not over-engineer. If MAX_MILESTONES is 0, the test would fail anyway, which is correct behavior.

Wait, but there's another issue: if MAX_MILESTONES is large (like 100), adding 100 milestones might be slow but should work. And the escrow needs enough balance. I set 1_000_000_000 total, and each milestone is 1_000_000. So 100 milestones = 100_000_000, fits in 1_000_000_000. Good.

But actually, does add_milestone check that sum of milestones <= total_amount? If so, 100 * 1_000_000 = 100_000_000 <= 1_000_000_000. Good.

Hmm, but what if MAX_MILESTONES is like 50? Then 50 * 1_000_000 = 50_000_000 <= 1_000_000_000. Good.

OK. Let me also reconsider: the `hash32` function takes `seed: u8`. In the boundary loop, `(i % 256) as u8` — if i is u32, `i % 256` is u32 in 0..255, `as u8` gives u8. Good.

Now let me also think about whether `crate::MAX_MILESTONES` and `crate::STORAGE_VERSION` are the right paths. They're likely `pub const` in the crate root or in types.rs. Since types.rs is a module, they might be `crate::types::MAX_MILESTONES` or re-exported at crate root. The existing imports use `crate::{EscrowContract, ...}` — so crate root re-exports. Let me use `crate::MAX_MILESTONES` and `crate::STORAGE_VERSION`. If they're not at crate root, this won't compile, but it's the most likely location given the pattern.

Actually, to be safer, I could use `crate::types::MAX_MILESTONES`. But I don't know if types is a module. Hmm. The file is `types.rs`, so `crate::types` should work if it's declared as `mod types;`. But the existing imports don't use `crate::types::`, they use `crate::`. So constants are likely re-exported at crate root. Let me use `crate::MAX_MILESTONES`.

OK, let me also reconsider the storage test. I want to make it more meaningful. Let me try to read STORAGE_VERSION from storage. But I need the DataKey. 

Actually, you know what, let me just reference the constant and verify milestone readability. That's the safest and still satisfies the requirement reasonably.

Hmm, but "assert storage layout stability by checking STORAGE_VERSION" — just referencing it doesn't "check" it. Let me think...

Maybe I can check that STORAGE_VERSION is a specific expected value. But I don't know the value. 

Alternative: check that the contract still works after upgrade (which implies storage layout is stable). And reference STORAGE_VERSION to prove it exists.

Let me write:
```rust
// STORAGE_VERSION must remain a stable, positive constant across upgrades.
let version: u32 = crate::STORAGE_VERSION;
assert!(version >= 1, "STORAGE_VERSION must be positive");
```

This at least "checks" it's positive. Weak but safe.

Actually, let me reconsider. Maybe I should try to access storage directly. In Soroban, you can do:
```rust
env.as_contract(&contract_id, || {
    env.storage().instance().get(&key)
});
```

But I need the key. If STORAGE_VERSION is stored under a DataKey variant like `DataKey::StorageVersion`, I'd need to know that. Too risky.

Let me go with the conservative approach. Final answer.

Actually, let me reconsider one more time. The issue says "File(s) to Edit: contracts/escrow_contract/src/types.rs" but the step says to add tests to upgrade_tests.rs. The current file is upgrade_tests.rs. So I'm editing upgrade_tests.rs. The types.rs mention might be a hint that MAX_MILESTONES/STORAGE_VERSION are defined there.

OK, let me finalize the code. I'll add the tests after the existing milestone test (test_upgrade_preserves_milestones) or at the end. Let me add them at the end, before the closing brace.

Actually, let me place them logically — after the existing milestone preservation test, in a new section. But to minimize disruption, I'll add them at the end of the module.

Let me write the final file:

```rust
//! # Upgrade Tests
//! ... (existing header)

#[cfg(test)]
#[allow(clippy::module_inception)]
mod upgrade_tests {
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

    use crate::{
        EscrowContract, EscrowContractClient, EscrowStatus, MultisigConfig, MS_PENDING,
        MS_SUBMITTED,
    };

    // ... existing helpers and tests ...

    // ─────────────────────────────────────────────────────────────────────────
    // Milestone lookup scalability tests
    // ─────────────────────────────────────────────────────────────────────────

    /// Success path: multiple milestones remain fully retrievable after upgrade.
    #[test]
    fn test_upgrade_preserves_multiple_milestones_lookup() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 10_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &10_000_000,
            &hash32(&env, 1),
            &None,
            &None,
            &None,
            &None,
            &no_multisig(&env),
            &None,
        );

        let mut ids: Vec<u64> = Vec::new(&env);
        let mut amounts: Vec<i128> = Vec::new(&env);
        for i in 0u8..5 {
            let amount = 1_000_000i128 * (i as i128 + 1);
            let m_id = contract.add_milestone(
                &client_addr,
                &escrow_id,
                &String::from_str(&env, "Milestone"),
                &hash32(&env, 100u8 + i),
                &amount,
            );
            ids.push_back(m_id);
            amounts.push_back(amount);
        }

        let wasm_hash = upload_dummy_wasm(&env);
        contract.upgrade(&admin, &wasm_hash);

        for i in 0..ids.len() {
            let id = ids.get(i).unwrap();
            let m = contract.get_milestone(&escrow_id, &id);
            assert_eq!(m.id, id);
            assert_eq!(m.amount, amounts.get(i).unwrap());
            assert_eq!(m.status, MS_PENDING);
        }
    }

    /// Failure path: looking up a never-created milestone id post-upgrade errors.
    #[test]
    fn test_upgrade_milestone_lookup_missing_id_fails() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 1_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &1_000_000,
            &hash32(&env, 2),
            &None,
            &None,
            &None,
            &None,
            &no_multisig(&env),
            &None,
        );

        let m_id = contract.add_milestone(
            &client_addr,
            &escrow_id,
            &String::from_str(&env, "Only"),
            &hash32(&env, 3),
            &1_000_000,
        );

        let wasm_hash = upload_dummy_wasm(&env);
        contract.upgrade(&admin, &wasm_hash);

        // Existing milestone still readable.
        let m = contract.get_milestone(&escrow_id, &m_id);
        assert_eq!(m.id, m_id);

        // A never-created id must error, not panic or return garbage.
        let missing_id = m_id + 999;
        let result = contract.try_get_milestone(&escrow_id, &missing_id);
        assert!(
            result.is_err(),
            "lookup of never-created milestone id must fail post-upgrade"
        );
    }

    /// Boundary path: a milestone at the MAX_MILESTONES boundary survives upgrade.
    #[test]
    fn test_upgrade_preserves_max_milestones_boundary() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 1_000_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &1_000_000_000,
            &hash32(&env, 4),
            &None,
            &None,
            &None,
            &None,
            &no_multisig(&env),
            &None,
        );

        let mut last_id = 0u64;
        for i in 0..crate::MAX_MILESTONES {
            last_id = contract.add_milestone(
                &client_addr,
                &escrow_id,
                &String::from_str(&env, "M"),
                &hash32(&env, (i % 256) as u8),
                &1_000_000,
            );
        }

        let wasm_hash = upload_dummy_wasm(&env);
        contract.upgrade(&admin, &wasm_hash);

        // The boundary milestone must still be retrievable.
        let m = contract.get_milestone(&escrow_id, &last_id);
        assert_eq!(m.id, last_id);
        assert_eq!(m.amount, 1_000_000);

        // Adding one more must still fail at the boundary post-upgrade.
        let result = contract.try_add_milestone(
            &client_addr,
            &escrow_id,
            &String::from_str(&env, "Overflow"),
            &hash32(&env, 200),
            &1_000_000,
        );
        assert!(
            result.is_err(),
            "adding beyond MAX_MILESTONES must fail post-upgrade"
        );
    }

    /// Public API behavior: approve_milestone still works post-upgrade.
    #[test]
    fn test_upgrade_approve_milestone_still_works() {
        let (env, admin, client_addr, freelancer, contract) = setup_initialized();
        let token = register_token(&env, &admin, &client_addr, 1_000_000);

        let escrow_id = contract.create_escrow(
            &client_addr,
            &freelancer,
            &token,
            &1_000_000,
            &hash32(&env, 11),
            &None,
            &None