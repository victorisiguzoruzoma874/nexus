# Swagger UI Testing Guide - Shift Creation Endpoints

## Access Swagger UI

1. **Start the server:**
   ```bash
   cd nexus
   cargo run
   ```

2. **Open Swagger UI in your browser:**
   ```
   http://localhost:8080/api/docs
   ```

## Available Shift Endpoints

### 1. POST /api/v1/shifts/preview
**Preview a shift before creation**

This endpoint allows you to preview how a shift will appear to workers without actually creating it.

**Test Payload (Normal In-Person Shift):**
```json
{
  "role_category": "doctor",
  "role_title": "Emergency Doctor",
  "specialty": "Emergency Medicine",
  "department": "Emergency Department",
  "shift_type": "in_person",
  "priority": "normal",
  "urgency_bonus_pct": null,
  "scheduled_start": "2026-05-16T08:00:00Z",
  "duration_hours": 8.0,
  "pay_type": "hourly_rate",
  "rate_kobo_per_hour": 800000,
  "fixed_rate_kobo": null,
  "stat_bonus_kobo": null,
  "shift_label": "Morning Shift: Emergency",
  "notes": "Standard morning coverage",
  "broadcast_consent_confirmed": true
}
```

**Expected Response (200 OK):**
```json
{
  "role_title": "Emergency Doctor",
  "specialty": "Emergency Medicine",
  "department": "Emergency Department",
  "shift_type": "InPerson",
  "priority": "Normal",
  "scheduled_start": "2026-05-16T08:00:00Z",
  "duration_hours": 8.0,
  "base_amount_kobo": 6400000,
  "stat_bonus_kobo": 0,
  "grand_total_kobo": 6400000,
  "virtual_link": null,
  "estimated_matches": 48
}
```

---

### 2. POST /api/v1/shifts
**Create a new shift**

This endpoint creates a new shift posting and broadcasts it to eligible workers.

**Test Payload (STAT Virtual Shift):**
```json
{
  "role_category": "doctor",
  "role_title": "Telemedicine Doctor",
  "specialty": "General Practice",
  "department": "Telemedicine Unit",
  "shift_type": "virtual",
  "priority": "stat",
  "urgency_bonus_pct": 20,
  "scheduled_start": "2026-05-15T18:30:00Z",
  "duration_hours": 4.0,
  "pay_type": "hourly_rate",
  "rate_kobo_per_hour": 1000000,
  "fixed_rate_kobo": null,
  "stat_bonus_kobo": 500000,
  "shift_label": "STAT Virtual Consultation",
  "notes": "Urgent telemedicine coverage needed",
  "broadcast_consent_confirmed": true
}
```

**Expected Response (201 Created):**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "hospital_id": "...",
  "role_category": "doctor",
  "role_title": "Telemedicine Doctor",
  "specialty": "General Practice",
  "department": "Telemedicine Unit",
  "shift_type": "virtual",
  "status": "open",
  "priority": "stat",
  "urgency_bonus_pct": 20,
  "scheduled_start": "2026-05-15T18:30:00Z",
  "duration_hours": 4.0,
  "scheduled_end": "2026-05-15T22:30:00Z",
  "grand_total_kobo": 4500000,
  "matched_clinicians_at_publish": 85,
  "broadcast_at": "2026-05-15T17:50:00Z",
  ...
}
```

---

### 3. GET /api/v1/shifts/{shift_id}
**Get shift details**

Retrieve detailed information about a specific shift.

**How to Test:**
1. First create a shift using POST /api/v1/shifts
2. Copy the `id` from the response
3. Use that ID in the GET endpoint

**Expected Response (200 OK):**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "hospital_id": "...",
  "role_title": "Emergency Doctor",
  "status": "open",
  ...
}
```

---

## Test Scenarios

### Scenario 1: Normal In-Person Shift
**Purpose:** Test basic shift creation with hourly rate

```json
{
  "role_category": "nurse",
  "role_title": "General Nurse",
  "specialty": "General Nursing",
  "department": "General Ward",
  "shift_type": "in_person",
  "priority": "normal",
  "scheduled_start": "2026-05-16T14:00:00Z",
  "duration_hours": 12.0,
  "pay_type": "hourly_rate",
  "rate_kobo_per_hour": 600000,
  "broadcast_consent_confirmed": true
}
```

**Expected:**
- ✅ Status: 201 Created
- ✅ Estimated matches: ~48 (5km radius)
- ✅ No virtual link
- ✅ Grand total: 7,200,000 kobo (₦72,000)

---

### Scenario 2: STAT Shift (Within 1 Hour)
**Purpose:** Test STAT shift validation and bonus

```json
{
  "role_category": "doctor",
  "role_title": "Emergency Doctor",
  "specialty": "Emergency Medicine",
  "shift_type": "in_person",
  "priority": "stat",
  "scheduled_start": "2026-05-15T18:30:00Z",
  "duration_hours": 8.0,
  "pay_type": "hourly_rate",
  "rate_kobo_per_hour": 800000,
  "stat_bonus_kobo": 500000,
  "broadcast_consent_confirmed": true
}
```

**Expected:**
- ✅ Status: 201 Created
- ✅ STAT bonus applied
- ✅ Grand total: 6,900,000 kobo (₦69,000)

---

### Scenario 3: Virtual Shift
**Purpose:** Test virtual shift with meeting link generation

```json
{
  "role_category": "doctor",
  "role_title": "Telemedicine Specialist",
  "specialty": "General Practice",
  "shift_type": "virtual",
  "priority": "normal",
  "scheduled_start": "2026-05-16T10:00:00Z",
  "duration_hours": 6.0,
  "pay_type": "hourly_rate",
  "rate_kobo_per_hour": 900000,
  "broadcast_consent_confirmed": true
}
```

**Expected:**
- ✅ Status: 201 Created
- ✅ Virtual meeting link generated
- ✅ Estimated matches: ~85 (no distance restriction)
- ✅ Grand total: 5,400,000 kobo (₦54,000)

---

### Scenario 4: Fixed Rate Shift
**Purpose:** Test fixed rate pay type

```json
{
  "role_category": "pharmacist",
  "role_title": "Night Pharmacist",
  "shift_type": "in_person",
  "priority": "normal",
  "scheduled_start": "2026-05-15T22:00:00Z",
  "duration_hours": 8.0,
  "pay_type": "fixed_rate",
  "fixed_rate_kobo": 5000000,
  "broadcast_consent_confirmed": true
}
```

**Expected:**
- ✅ Status: 201 Created
- ✅ Grand total: 5,000,000 kobo (₦50,000)

---

## Error Testing

### Test 1: Missing Required Field
**Purpose:** Test validation error handling

```json
{
  "role_category": "doctor",
  "role_title": "",
  "shift_type": "in_person",
  "priority": "normal",
  "scheduled_start": "2026-05-16T08:00:00Z",
  "duration_hours": 8.0,
  "pay_type": "hourly_rate",
  "broadcast_consent_confirmed": true
}
```

**Expected:**
- ❌ Status: 422 Unprocessable Entity
- ❌ Error: "Role title is required"

---

### Test 2: STAT Shift Beyond 1 Hour
**Purpose:** Test STAT time validation

```json
{
  "role_category": "doctor",
  "role_title": "Emergency Doctor",
  "shift_type": "in_person",
  "priority": "stat",
  "scheduled_start": "2026-05-16T20:00:00Z",
  "duration_hours": 8.0,
  "pay_type": "hourly_rate",
  "rate_kobo_per_hour": 800000,
  "stat_bonus_kobo": 500000,
  "broadcast_consent_confirmed": true
}
```

**Expected:**
- ❌ Status: 422 Unprocessable Entity
- ❌ Error: "STAT shifts must start within one hour"

---

### Test 3: Missing Hourly Rate
**Purpose:** Test pay type validation

```json
{
  "role_category": "nurse",
  "role_title": "General Nurse",
  "shift_type": "in_person",
  "priority": "normal",
  "scheduled_start": "2026-05-16T08:00:00Z",
  "duration_hours": 8.0,
  "pay_type": "hourly_rate",
  "broadcast_consent_confirmed": true
}
```

**Expected:**
- ❌ Status: 422 Unprocessable Entity
- ❌ Error: "Hourly rate is required for hourly pay type"

---

### Test 4: STAT Without Bonus
**Purpose:** Test STAT bonus requirement

```json
{
  "role_category": "doctor",
  "role_title": "Emergency Doctor",
  "shift_type": "in_person",
  "priority": "stat",
  "scheduled_start": "2026-05-15T18:30:00Z",
  "duration_hours": 8.0,
  "pay_type": "hourly_rate",
  "rate_kobo_per_hour": 800000,
  "broadcast_consent_confirmed": true
}
```

**Expected:**
- ❌ Status: 422 Unprocessable Entity
- ❌ Error: "STAT shifts require urgency bonus or stat bonus"

---

### Test 5: Duplicate Shift
**Purpose:** Test duplicate detection

**Steps:**
1. Create a shift with specific role_title and scheduled_start
2. Immediately create another shift with same details
3. Second request should fail

**Expected:**
- ❌ Status: 409 Conflict
- ❌ Error: "Similar shift already exists."

---

### Test 6: Missing Broadcast Consent
**Purpose:** Test consent validation

```json
{
  "role_category": "doctor",
  "role_title": "Emergency Doctor",
  "shift_type": "in_person",
  "priority": "normal",
  "scheduled_start": "2026-05-16T08:00:00Z",
  "duration_hours": 8.0,
  "pay_type": "hourly_rate",
  "rate_kobo_per_hour": 800000,
  "broadcast_consent_confirmed": false
}
```

**Expected:**
- ❌ Status: 422 Unprocessable Entity
- ❌ Error: "Broadcast consent must be confirmed"

---

### Test 7: Unapproved Hospital
**Purpose:** Test hospital approval requirement

**Note:** This test requires using a hospital_id that is not approved. In production, the hospital_id would come from the JWT token.

**Expected:**
- ❌ Status: 403 Forbidden
- ❌ Error: "Only approved hospitals can create shifts. Please complete your registration and wait for approval."

---

## New Features

### Hospital Name in Shifts
All shifts now include the hospital name that created them:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "hospital_id": "...",
  "hospital_name": "Lagos University Teaching Hospital",  // NEW
  "role_title": "Emergency Doctor",
  ...
}
```

### Hospital Approval Check
Only approved hospitals can create shifts. Unapproved hospitals will receive a 403 Forbidden error.

**Approval Status Flow:**
1. Hospital registers → Status: Pending
2. Admin reviews → Status: Approved or Rejected
3. Only Approved hospitals can create shifts

---

## Swagger UI Tips

### Using the "Try it out" Feature
1. Click on an endpoint to expand it
2. Click the **"Try it out"** button
3. Edit the request body in the text area
4. Click **"Execute"** to send the request
5. View the response below

### Understanding Response Codes
- **200 OK** - Request successful (GET, Preview)
- **201 Created** - Resource created successfully (POST)
- **403 Forbidden** - Hospital not approved to create shifts
- **422 Unprocessable Entity** - Validation error
- **409 Conflict** - Duplicate resource
- **404 Not Found** - Resource doesn't exist
- **500 Internal Server Error** - Server error

### Viewing Schemas
- Click on **"Schemas"** at the bottom of the Swagger UI
- View all available data models
- See required fields and data types

### Testing Workflow
1. **Preview First** - Use `/api/v1/shifts/preview` to validate your payload
2. **Create Shift** - Use `/api/v1/shifts` to create the shift
3. **Get Details** - Use `/api/v1/shifts/{shift_id}` to verify creation

---

## Currency Conversion Reference

All amounts are in **kobo** (1/100 of Nigerian Naira):

| Kobo | Naira |
|------|-------|
| 100,000 | ₦1,000 |
| 500,000 | ₦5,000 |
| 600,000 | ₦6,000 |
| 800,000 | ₦8,000 |
| 1,000,000 | ₦10,000 |
| 5,000,000 | ₦50,000 |
| 6,400,000 | ₦64,000 |

---

## Troubleshooting

### Server Not Starting
```bash
# Check if port 8080 is in use
lsof -i :8080

# Kill the process if needed
kill -9 <PID>

# Restart the server
cargo run
```

### Swagger UI Not Loading
- Ensure server is running: `cargo run`
- Check URL: `http://localhost:8080/api/docs`
- Clear browser cache
- Try incognito/private mode

### Database Errors
```bash
# Check database connection
psql -U postgres -d nexuscare

# Run migrations
sqlx migrate run
```

### Validation Errors
- Check all required fields are present
- Verify data types match schema
- Ensure enums use correct values (lowercase with underscores)
- Confirm scheduled_start is in future
- Verify STAT shifts start within 1 hour

---

## Quick Test Checklist

- [ ] Preview a normal shift
- [ ] Create an in-person shift
- [ ] Create a virtual shift
- [ ] Create a STAT shift (within 1 hour)
- [ ] Create a fixed rate shift
- [ ] Test missing required field error
- [ ] Test STAT time validation error
- [ ] Test duplicate shift detection
- [ ] Test unapproved hospital error (403)
- [ ] Verify hospital name in shift response
- [ ] Get shift details by ID
- [ ] Verify virtual meeting link generation
- [ ] Verify compensation calculations
- [ ] Check estimated matches (48 vs 85)

---

**Happy Testing! 🚀**

For more details, see:
- `SHIFT_CREATION_FEATURE.md` - Complete feature documentation
- `QUICK_START_GUIDE.md` - Quick reference guide
- `IMPLEMENTATION_SUMMARY.md` - Implementation overview
