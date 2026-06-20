# Swagger Integration Summary

## What Was Added

### 1. OpenAPI Documentation Attributes

Added `#[utoipa::path]` attributes to all shift endpoints in `src/handlers/shifts.rs`:

- **POST /api/v1/shifts** - Create a new shift
- **POST /api/v1/shifts/preview** - Preview shift before creation
- **GET /api/v1/shifts/{shift_id}** - Get shift details

### 2. Schema Annotations

Added `ToSchema` derive to all shift-related models in `src/models/shift.rs`:

- `Shift` - Main shift model
- `CreateShiftRequest` - Request payload
- `ShiftStatus` - Enum
- `ShiftPriority` - Enum
- `ShiftType` - Enum
- `RoleCategory` - Enum
- `PayType` - Enum

### 3. Response Models

Created new response models in `src/handlers/shifts.rs`:

- `ShiftPreviewResponse` - Preview response with serializable fields
- `ErrorResponse` - Error response structure
- `ErrorDetail` - Error detail structure

### 4. OpenAPI Configuration

Updated `src/routes/app_routes.rs` to include:

**Paths:**
- `crate::handlers::shifts::create_shift`
- `crate::handlers::shifts::preview_shift`
- `crate::handlers::shifts::get_shift`

**Schemas:**
- All shift models and enums
- Request and response types
- Error response types

**Tags:**
- Added "shifts" tag for shift-related endpoints

**API Info:**
- Updated title to "NexusCare Hospital Management API"
- Updated description to include shift creation

## Files Modified

1. `src/handlers/shifts.rs` - Added OpenAPI attributes and response models
2. `src/models/shift.rs` - Added ToSchema derives
3. `src/routes/app_routes.rs` - Updated OpenAPI configuration

## How to Access

1. **Start the server:**
   ```bash
   cd nexus
   cargo run
   ```

2. **Open Swagger UI:**
   ```
   http://localhost:8080/api/docs
   ```

3. **View OpenAPI JSON:**
   ```
   http://localhost:8080/api/openapi.json
   ```

## Available Documentation

### Swagger UI Features

- **Interactive API Testing** - Try out endpoints directly from the browser
- **Request/Response Examples** - See example payloads and responses
- **Schema Definitions** - View all data models and their fields
- **Validation Rules** - See required fields and constraints
- **Error Responses** - Understand possible error scenarios

### Endpoint Documentation

Each endpoint includes:
- HTTP method and path
- Request body schema
- Response codes and schemas
- Description and summary
- Tag for grouping

### Schema Documentation

Each model includes:
- Field names and types
- Required vs optional fields
- Enum values
- Nested object structures

## Testing Guide

See `SWAGGER_TESTING_GUIDE.md` for:
- Step-by-step testing instructions
- Example payloads for each scenario
- Error testing scenarios
- Expected responses
- Troubleshooting tips

## Example Usage

### 1. Preview a Shift

**Endpoint:** POST /api/v1/shifts/preview

**Request:**
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
  "broadcast_consent_confirmed": true
}
```

**Response (200 OK):**
```json
{
  "role_title": "Emergency Doctor",
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

### 2. Create a Shift

**Endpoint:** POST /api/v1/shifts

**Request:** (Same as preview)

**Response (201 Created):**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "hospital_id": "...",
  "role_title": "Emergency Doctor",
  "status": "open",
  "priority": "normal",
  "grand_total_kobo": 6400000,
  "matched_clinicians_at_publish": 48,
  "broadcast_at": "2026-05-15T17:50:00Z",
  ...
}
```

### 3. Get Shift Details

**Endpoint:** GET /api/v1/shifts/{shift_id}

**Response (200 OK):**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "hospital_id": "...",
  "role_title": "Emergency Doctor",
  "status": "open",
  ...
}
```

## Validation in Swagger

Swagger UI will show validation errors for:
- Missing required fields
- Invalid enum values
- Invalid data types
- Constraint violations (e.g., duration_hours range)

## Response Codes

| Code | Description | When |
|------|-------------|------|
| 200 | OK | Successful GET or preview |
| 201 | Created | Shift created successfully |
| 422 | Unprocessable Entity | Validation error |
| 409 | Conflict | Duplicate shift |
| 404 | Not Found | Shift ID not found |
| 500 | Internal Server Error | Server error |

## Schema Definitions

### CreateShiftRequest
- role_category: RoleCategory (required)
- role_title: string (required, min: 2, max: 255)
- specialty: string (optional)
- department: string (optional, max: 255)
- shift_type: ShiftType (required)
- priority: ShiftPriority (required)
- urgency_bonus_pct: integer (optional, 0-100)
- scheduled_start: datetime (required)
- duration_hours: number (required, 0.5-24)
- pay_type: PayType (required)
- rate_kobo_per_hour: integer (optional)
- fixed_rate_kobo: integer (optional)
- stat_bonus_kobo: integer (optional)
- shift_label: string (optional, max: 100)
- notes: string (optional, max: 1000)
- broadcast_consent_confirmed: boolean (required)

### Shift
- id: UUID
- hospital_id: UUID
- role_category: RoleCategory
- role_title: string
- specialty: string (optional)
- department: string (optional)
- shift_type: ShiftType
- status: ShiftStatus
- priority: ShiftPriority
- urgency_bonus_pct: integer (optional)
- scheduled_start: datetime
- duration_hours: number
- scheduled_end: datetime
- actual_start: datetime (optional)
- actual_end: datetime (optional)
- assigned_clinician_id: UUID (optional)
- rate_kobo_per_hour: integer (optional)
- fixed_rate_kobo: integer (optional)
- pay_type: PayType
- stat_bonus_kobo: integer (optional)
- effective_rate_kobo_per_hour: integer (optional)
- grand_total_kobo: integer (optional)
- shift_label: string (optional)
- job_description: string (optional)
- draft_quality_score: integer (optional)
- notes: string (optional)
- created_by: UUID
- broadcast_consent_confirmed: boolean
- matched_clinicians_at_publish: integer (optional)
- broadcast_at: datetime (optional)
- billing_triggered_at: datetime (optional)
- created_at: datetime
- updated_at: datetime

### Enums

**ShiftStatus:**
- open
- upcoming
- in_progress
- completed
- cancelled
- no_show

**ShiftPriority:**
- normal
- stat
- urgent

**ShiftType:**
- in_person
- virtual

**RoleCategory:**
- doctor
- nurse
- pharmacist
- lab_technician
- radiographer
- physiotherapist
- other

**PayType:**
- hourly_rate
- fixed_rate

## Benefits

### For Developers
- Interactive API testing without external tools
- Clear documentation of all endpoints
- Type-safe request/response validation
- Easy to understand API structure

### For Testers
- No need for Postman or curl
- Visual interface for testing
- Immediate feedback on validation errors
- Example payloads provided

### For Frontend Developers
- Clear API contract
- Type definitions for TypeScript generation
- Example requests and responses
- Error handling documentation

## Next Steps

1. **Test all endpoints** using Swagger UI
2. **Verify validation** works as expected
3. **Check error responses** are clear
4. **Generate TypeScript types** from OpenAPI spec (optional)
5. **Share API documentation** with team

## OpenAPI Spec Export

You can export the OpenAPI specification for use with other tools:

```bash
# Get the OpenAPI JSON
curl http://localhost:8080/api/openapi.json > openapi.json

# Use with code generators
npx @openapitools/openapi-generator-cli generate \
  -i openapi.json \
  -g typescript-axios \
  -o ./generated-client
```

## Troubleshooting

### Swagger UI Not Loading
- Check server is running: `cargo run`
- Verify URL: `http://localhost:8080/api/docs`
- Check browser console for errors

### Schemas Not Showing
- Ensure all models have `ToSchema` derive
- Check they're included in `components(schemas(...))` in routes
- Rebuild: `cargo clean && cargo build`

### Endpoints Not Appearing
- Verify `#[utoipa::path]` attribute is present
- Check endpoint is included in `paths(...)` in routes
- Ensure handler function is public

### Validation Not Working
- Check `Validate` derive is present on request models
- Verify validation rules in struct fields
- Test with invalid data to confirm

## Resources

- **Utoipa Documentation:** https://docs.rs/utoipa/
- **OpenAPI Specification:** https://swagger.io/specification/
- **Swagger UI:** https://swagger.io/tools/swagger-ui/

---

**Swagger Integration Complete! ✅**

Access the interactive API documentation at: http://localhost:8080/api/docs
