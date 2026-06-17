# SafeHaven Deposit (Virtual Account) — Blocked, pending SafeHaven support

**Status:** the deposit flow code is complete and correct; blocked by a SafeHaven **account/KYC configuration** issue, not by our implementation.

## Symptom

`POST /api/v1/wallet/deposits` → `WalletService::request_deposit` → SafeHaven `POST /virtual-accounts`
returns:

```json
{ "statusCode": 400, "message": "Invalid settlement bank code. Please try again." }
```

(HTTP envelope is `201 Created` but the body carries `statusCode: 400`.)

## What we send (matches SafeHaven's "Create Virtual Account" doc example exactly)

```json
{
  "validFor": 86400,
  "callbackUrl": "https://<public>/api/v1/webhooks/safehaven",
  "settlementAccount": { "bankCode": "090286" },
  "amountControl": "OverPayment",
  "amount": 1000,
  "externalReference": "dep_<uuid>"
}
```

- `090286` = SafeHaven MFB's own bank code (confirmed via `GET /transfers/banks` and the docs example).
- Also tried `999240` — same rejection.
- Confirmed the exact outgoing payload + response via debug logging in `safehaven.rs::post_authed`.

## Why this is NOT our code

- Request is byte-for-byte the documented shape (settlementAccount carries **only** bankCode).
- Everything else on the same SafeHaven account works **live**: OAuth, BVN/NIN identity verify, sub-account creation (`5011243906`), name-enquiry, and the payout transfer path.

## Leading hypothesis

The SafeHaven account (settlement `0107343367`, client_id `44ec5a3c…`) is **KYC Level 1**. Virtual-account collection / settlement likely requires a higher KYC tier and/or `090286` to be enabled as a settlement bank on the merchant profile.

## To resume

Ask SafeHaven support:
1. What settlement `bankCode` should this account use for virtual accounts?
2. Does virtual-account creation require a KYC tier above Level 1?

Once confirmed, set `SAFEHAVEN_BANK_CODE` (and, if needed, re-add a settlement account field) and re-run `POST /api/v1/wallet/deposits`. No code change is expected beyond possibly the bank code value.
