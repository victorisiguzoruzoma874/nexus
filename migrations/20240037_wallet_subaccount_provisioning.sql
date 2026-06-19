-- Transient state for the two-step SafeHaven sub-account provisioning:
-- initiate stores the fresh verification id (+ BVN) so the provision step can
-- pass identityId + otp to /accounts/v2/subaccount.
ALTER TABLE hospital_wallets
    ADD COLUMN provisioning_identity_id TEXT,
    ADD COLUMN provisioning_bvn         TEXT;
