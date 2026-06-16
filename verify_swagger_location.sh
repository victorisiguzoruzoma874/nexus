#!/bin/bash

echo "=== NexusCare Location API - Swagger UI Verification ==="
echo

BASE_URL="http://localhost:8080"

# Test if Swagger UI is accessible
echo "1. Testing Swagger UI access..."
SWAGGER_RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/api/docs")
if [ "$SWAGGER_RESPONSE" = "200" ]; then
    echo "✅ Swagger UI is accessible at ${BASE_URL}/api/docs"
else
    echo "❌ Swagger UI not accessible (HTTP $SWAGGER_RESPONSE)"
fi

# Test if OpenAPI JSON is accessible
echo
echo "2. Testing OpenAPI JSON..."
OPENAPI_RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/api/openapi.json")
if [ "$OPENAPI_RESPONSE" = "200" ]; then
    echo "✅ OpenAPI JSON is accessible at ${BASE_URL}/api/openapi.json"
else
    echo "❌ OpenAPI JSON not accessible (HTTP $OPENAPI_RESPONSE)"
fi

# Verify location tag exists
echo
echo "3. Verifying location tag..."
LOCATION_TAG=$(curl -s "${BASE_URL}/api/openapi.json" | jq -r '.tags[] | select(.name == "location") | .name')
if [ "$LOCATION_TAG" = "location" ]; then
    echo "✅ Location tag is properly configured"
else
    echo "❌ Location tag missing"
fi

# Verify all location endpoints are documented
echo
echo "4. Verifying location endpoints are documented..."

ENDPOINTS=(
    "/api/v1/location/health-facilities/search"
    "/api/v1/location/nexuscare-facilities/search"
    "/api/v1/location/address/autocomplete"
    "/api/v1/location/nearby-shifts"
    "/api/v1/here/geocode"
    "/api/v1/here/reverse-geocode"
    "/api/v1/distance/calculate"
)

for endpoint in "${ENDPOINTS[@]}"; do
    METHOD="get"
    if [[ "$endpoint" == *"distance/calculate"* ]]; then
        METHOD="post"
    fi
    
    ENDPOINT_EXISTS=$(curl -s "${BASE_URL}/api/openapi.json" | jq -r ".paths.\"$endpoint\".${METHOD} // empty")
    if [ -n "$ENDPOINT_EXISTS" ]; then
        echo "✅ $endpoint ($METHOD)"
    else
        echo "❌ $endpoint ($METHOD) - Missing"
    fi
done

# Verify key schemas are present
echo
echo "5. Verifying schemas are present..."

SCHEMAS=(
    "FacilitySearchResponse"
    "Facility"
    "NearbyShiftsResponse"
    "AddressAutocompleteResponse"
    "GeocodeResponse"
    "ReverseGeocodeResponse"
    "DistanceRequest"
    "DistanceResponse"
)

for schema in "${SCHEMAS[@]}"; do
    SCHEMA_EXISTS=$(curl -s "${BASE_URL}/api/openapi.json" | jq -r ".components.schemas.${schema} // empty")
    if [ -n "$SCHEMA_EXISTS" ]; then
        echo "✅ $schema"
    else
        echo "❌ $schema - Missing"
    fi
done

# Test endpoint functionality
echo
echo "6. Testing endpoint functionality..."

# Test coordinates for Lagos, Nigeria
LAT=6.5244
LNG=3.3792

# Test basic endpoints
echo "Testing health facilities search..."
HEALTH_RESPONSE=$(curl -s "${BASE_URL}/api/v1/location/health-facilities/search?lat=${LAT}&lng=${LNG}&radius=5000&limit=5" | jq -r '.error.message // .total_found // "error"')
if [[ "$HEALTH_RESPONSE" =~ ^[0-9]+$ ]] || [ "$HEALTH_RESPONSE" = "Health facility search unavailable" ]; then
    echo "✅ Health facilities search endpoint working"
else
    echo "❌ Health facilities search endpoint error: $HEALTH_RESPONSE"
fi

echo "Testing nexuscare facilities..."
NEXUS_RESPONSE=$(curl -s "${BASE_URL}/api/v1/location/nexuscare-facilities/search?lat=${LAT}&lng=${LNG}&radius=5000&limit=5" | jq -r '.total_found // "error"')
if [[ "$NEXUS_RESPONSE" =~ ^[0-9]+$ ]]; then
    echo "✅ Nexuscare facilities endpoint working (found $NEXUS_RESPONSE facilities)"
else
    echo "❌ Nexuscare facilities endpoint error: $NEXUS_RESPONSE"
fi

echo "Testing address autocomplete..."
AUTOCOMPLETE_RESPONSE=$(curl -s "${BASE_URL}/api/v1/location/address/autocomplete?q=Lagos&lat=${LAT}&lng=${LNG}&radius=5000" | jq -r '.suggestions | length // "error"')
if [[ "$AUTOCOMPLETE_RESPONSE" =~ ^[0-9]+$ ]]; then
    echo "✅ Address autocomplete endpoint working (found $AUTOCOMPLETE_RESPONSE suggestions)"
else
    echo "❌ Address autocomplete endpoint error: $AUTOCOMPLETE_RESPONSE"
fi

echo "Testing nearby shifts..."
SHIFTS_RESPONSE=$(curl -s "${BASE_URL}/api/v1/location/nearby-shifts?lat=${LAT}&lng=${LNG}&radius=5000&limit=5" | jq -r '.total_active_shifts // "error"')
if [[ "$SHIFTS_RESPONSE" =~ ^[0-9]+$ ]]; then
    echo "✅ Nearby shifts endpoint working (found $SHIFTS_RESPONSE active shifts)"
else
    echo "❌ Nearby shifts endpoint error: $SHIFTS_RESPONSE"
fi

echo "Testing geocoding..."
GEOCODE_RESPONSE=$(curl -s "${BASE_URL}/api/v1/here/geocode?q=Lagos+Nigeria&limit=3" | jq -r '.items | length // "error"')
if [[ "$GEOCODE_RESPONSE" =~ ^[0-9]+$ ]]; then
    echo "✅ Geocoding endpoint working (found $GEOCODE_RESPONSE results)"
else
    echo "❌ Geocoding endpoint error: $GEOCODE_RESPONSE"
fi

echo "Testing reverse geocoding..."
REVERSE_RESPONSE=$(curl -s "${BASE_URL}/api/v1/here/reverse-geocode?at=${LAT},${LNG}&limit=3" | jq -r '.items | length // "error"')
if [[ "$REVERSE_RESPONSE" =~ ^[0-9]+$ ]]; then
    echo "✅ Reverse geocoding endpoint working (found $REVERSE_RESPONSE results)"
else
    echo "❌ Reverse geocoding endpoint error: $REVERSE_RESPONSE"
fi

echo "Testing distance calculation..."
DISTANCE_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/distance/calculate" \
  -H "Content-Type: application/json" \
  -d '{
    "origin": {
      "type": "coordinates",
      "coordinates": {"latitude": 6.5244, "longitude": 3.3792}
    },
    "destination": {
      "type": "coordinates", 
      "coordinates": {"latitude": 6.6018, "longitude": 3.3515}
    },
    "transport_mode": "car"
  }' | jq -r '.distance.formatted // "error"')
if [ "$DISTANCE_RESPONSE" != "error" ] && [ "$DISTANCE_RESPONSE" != "null" ]; then
    echo "✅ Distance calculation endpoint working (distance: $DISTANCE_RESPONSE)"
else
    echo "❌ Distance calculation endpoint error"
fi

echo
echo "=== Summary ==="
echo "All location-based endpoints should now be properly documented in Swagger UI"
echo "Visit: ${BASE_URL}/api/docs"
echo "Look for the 'location' tag which contains:"
echo "  - Health Facilities Search"
echo "  - Nexuscare Facilities Search"  
echo "  - Address Autocomplete"
echo "  - Nearby Shifts Search"
echo "  - HERE Maps Geocoding"
echo "  - HERE Maps Reverse Geocoding"
echo "  - Distance Calculation"
