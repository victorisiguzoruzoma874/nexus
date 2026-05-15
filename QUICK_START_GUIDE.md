# Shift Creation Feature - Quick Start Guide

## Overview
This guide helps you quickly understand and use the new shift creation feature.

## Key Features

### ✅ What's New
1. **Shift Preview** - Preview shifts before publishing
2. **STAT Shift Validation** - Automatic validation for urgent shifts
3. **Virtual Meeting Links** - Auto-generated for virtual shifts
4. **Duplicate Detection** - Prevents duplicate shift creation
5. **Worker Notifications** - Automatic broadcast to eligible workers
6. **Distance-Based Matching** - 5km radius for in-person shifts

## API Quick Reference

### Create a Shift
```bash
POST /api/v1/shifts
Content-Type: application/json

{
  "role_category": "doctor",
  "role_title": "Emergency Doctor",
  "specialty": "Emergency Medicine",
  "department": "Emergency Department",
  "shift_type": "in_person",           # or "virtual"
  "priority": "normal",                 # or "stat", "urgent"
  "scheduled_start": "2026-05-15T20:00:00Z",
  "duration_hours": 8.0,
  "pay_type": "hourly_rate",           # or "fixed_rate"
  "rate_kobo_per_hour": 800000,        # ₦8,000/hr
  "stat_bonus_kobo": 500000,           # ₦5,000 bonus (optional)
  "broadcast_consent_confirmed": true
}
```

### Preview a Shift
```bash
POST /api/v1/shifts/preview
Content-Type: application/json

# Same payload as create shift
```

### Get Shift Details
```bash
GET /api/v1/shifts/{shift_id}
```

## Validation Rules

### Required Fields
- ✅ `role_category` - Must be valid enum
- ✅ `role_title` - Cannot be empty
- ✅ `shift_type` - in_person or virtual
- ✅ `priority` - normal, stat, or urgent
- ✅ `scheduled_start` - Future datetime
- ✅ `duration_hours` - Between 0.5 and 24
- ✅ `pay_type` - hourly_rate or fixed_rate
- ✅ `broadcast_consent_confirmed` - Must be true

### Pay Type Rules
**Hourly Rate:**
- ✅ `rate_kobo_per_hour` required
- ❌ `fixed_rate_kobo` not used

**Fixed Rate:**
- ✅ `fixed_rate_kobo` required
- ❌ `rate_kobo_per_hour` not used

### STAT Shift Rules
When `priority = "stat"`:
- ✅ `scheduled_start` must be within 1 hour
- ✅ Must have `stat_bonus_kobo` OR `urgency_bonus_pct`
- ❌ Cannot start more than 1 hour in future

### Urgent Shift Rules
When `priority = "urgent"`:
- ✅ Must have `stat_bonus_kobo` OR `urgency_bonus_pct`

## Examples

### Example 1: Normal In-Person Shift
```json
{
  "role_category": "nurse",
  "role_title": "General Nurse",
  "specialty": "General Nursing",
  "shift_type": "in_person",
  "priority": "normal",
  "scheduled_start": "2026-05-16T08:00:00Z",
  "duration_hours": 12.0,
  "pay_type": "hourly_rate",
  "rate_kobo_per_hour": 600000,
  "broadcast_consent_confirmed": true
}
```

### Example 2: STAT Virtual Shift
```json
{
  "role_category": "doctor",
  "role_title": "Telemedicine Doctor",
  "specialty": "General Practice",
  "shift_type": "virtual",
  "priority": "stat",
  "scheduled_start": "2026-05-15T18:30:00Z",  # Within 1 hour
  "duration_hours": 4.0,
  "pay_type": "hourly_rate",
  "rate_kobo_per_hour": 1000000,
  "stat_bonus_kobo": 500000,
  "broadcast_consent_confirmed": true
}
```

### Example 3: Fixed Rate Shift
```json
{
  "role_category": "pharmacist",
  "role_title": "Night Pharmacist",
  "shift_type": "in_person",
  "priority": "normal",
  "scheduled_start": "2026-05-15T22:00:00Z",
  "duration_hours": 8.0,
  "pay_type": "fixed_rate",
  "fixed_rate_kobo": 5000000,  # ₦50,000 flat rate
  "broadcast_consent_confirmed": true
}
```

## Error Messages

### Common Errors

**"Role title is required"**
- Fix: Provide non-empty `role_title`

**"Hourly rate is required for hourly pay type"**
- Fix: Set `rate_kobo_per_hour` when `pay_type = "hourly_rate"`

**"STAT shifts must start within one hour"**
- Fix: Set `scheduled_start` to within 1 hour for STAT priority

**"STAT shifts require urgency bonus or stat bonus"**
- Fix: Add `stat_bonus_kobo` or `urgency_bonus_pct` for STAT shifts

**"Similar shift already exists."**
- Fix: Check for duplicate shifts with same role_title and scheduled_start

**"Broadcast consent must be confirmed"**
- Fix: Set `broadcast_consent_confirmed: true`

## Response Codes

| Code | Meaning | Action |
|------|---------|--------|
| 201 | Created | Shift successfully created |
| 200 | OK | Preview generated successfully |
| 422 | Validation Error | Check error message for details |
| 409 | Conflict | Duplicate shift detected |
| 404 | Not Found | Shift ID doesn't exist |
| 500 | Server Error | Contact support |

## Testing

### Run Tests
```bash
cd nexus
cargo test --test shift_creation_tests
```

### Expected Output
```
running 19 tests
✅ 16 passed
⏸️ 3 ignored
❌ 0 failed
```

## Shift Types Comparison

| Feature | In-Person | Virtual |
|---------|-----------|---------|
| Distance Restriction | 5km radius | None |
| Meeting Link | No | Auto-generated |
| GPS Clock-in | Required | Not required |
| Matched Workers | ~48 | ~85 |

## Priority Levels

| Priority | Badge Color | Bonus Required | Time Restriction |
|----------|-------------|----------------|------------------|
| Normal | Gray | No | None |
| STAT | Orange | Yes | Within 1 hour |
| Urgent | Red/Yellow | Yes | None |

## Compensation Calculation

### Hourly Rate
```
Base = rate_kobo_per_hour × duration_hours
Total = Base + stat_bonus_kobo
```

Example:
```
Rate: ₦8,000/hr
Duration: 8 hours
STAT Bonus: ₦5,000

Base = 800,000 × 8 = 6,400,000 kobo (₦64,000)
Total = 6,400,000 + 500,000 = 6,900,000 kobo (₦69,000)
```

### Fixed Rate
```
Total = fixed_rate_kobo + stat_bonus_kobo
```

Example:
```
Fixed Rate: ₦50,000
STAT Bonus: ₦5,000

Total = 5,000,000 + 500,000 = 5,500,000 kobo (₦55,000)
```

## Workflow

1. **Fill Form** → Administrator enters shift details
2. **Preview** → Click preview to see how shift appears
3. **Validate** → System validates all fields
4. **Check Duplicates** → System checks for similar shifts
5. **Create** → Shift is created in database
6. **Generate Link** → Virtual link created (if virtual)
7. **Match Workers** → System finds eligible workers
8. **Broadcast** → Notifications sent to workers
9. **Confirm** → Administrator receives confirmation

## Tips & Best Practices

### ✅ Do's
- Preview shifts before creating
- Use STAT priority only for true emergencies
- Provide clear role titles and descriptions
- Set appropriate compensation rates
- Confirm broadcast consent

### ❌ Don'ts
- Don't create duplicate shifts
- Don't use STAT for shifts starting >1 hour away
- Don't forget to set bonus for STAT/Urgent shifts
- Don't leave required fields empty
- Don't skip preview step

## Support

### Documentation
- Full documentation: `SHIFT_CREATION_FEATURE.md`
- Implementation details: `IMPLEMENTATION_SUMMARY.md`

### Troubleshooting
1. Check error message for specific issue
2. Verify all required fields are filled
3. Ensure STAT shifts meet time requirements
4. Check for duplicate shifts
5. Review validation rules above

### Debug Mode
```bash
RUST_LOG=debug cargo run
```

## Currency Note
All amounts are in **kobo** (1/100 of Nigerian Naira):
- 100 kobo = ₦1
- 1,000 kobo = ₦10
- 100,000 kobo = ₦1,000
- 1,000,000 kobo = ₦10,000

Example: `800000` kobo = ₦8,000

---

**Quick Start Complete!** 🚀

For detailed information, see `SHIFT_CREATION_FEATURE.md`
