#!/bin/bash

BASE_URL="http://localhost:8080"

echo "Testing Location Endpoints for Swagger UI..."
echo "============================================="

# Test 1: Health Facilities Search
echo "1. Testing /api/v1/location/health-facilities/search"
curl -s -X GET "${BASE_URL}/api/v1/location/health-facilities/search?lat=6.5244&lng=3.3792&radius=5000&limit=10" \
  -H "Accept: application/json" | jq . | head -20

echo -e "\n"

# Test 2: Nexuscare Facilities Search
echo "2. Testing /api/v1/location/nexuscare-facilities/search"
curl -s -X GET "${BASE_URL}/api/v1/location/nexuscare-facilities/search?lat=6.5244&lng=3.3792&radius=5000&limit=10" \
  -H "Accept: application/json" | jq . | head -20

echo -e "\n"

# Test 3: Address Autocomplete
echo "3. Testing /api/v1/location/address/autocomplete"
curl -s -X GET "${BASE_URL}/api/v1/location/address/autocomplete?q=Lagos&lat=6.5244&lng=3.3792&radius=5000" \
  -H "Accept: application/json" | jq . | head -20

echo -e "\n"

# Test 4: Nearby Shifts
echo "4. Testing /api/v1/location/nearby-shifts"
curl -s -X GET "${BASE_URL}/api/v1/location/nearby-shifts?lat=6.5244&lng=3.3792&radius=5000&limit=10" \
  -H "Accept: application/json" | jq . | head -20

echo -e "\n"

# Test 5: HERE Maps Geocoding
echo "5. Testing /api/v1/here/geocode"
curl -s -X GET "${BASE_URL}/api/v1/here/geocode?q=Lagos+Nigeria&limit=5" \
  -H "Accept: application/json" | jq . | head -20

echo -e "\n"

# Test 6: HERE Maps Reverse Geocoding
echo "6. Testing /api/v1/here/reverse-geocode"
curl -s -X GET "${BASE_URL}/api/v1/here/reverse-geocode?at=6.5244,3.3792&limit=5" \
  -H "Accept: application/json" | jq . | head -20

echo -e "\n"

# Test 7: Distance calculation
echo "7. Testing /api/v1/distance/calculate"
curl -s -X POST "${BASE_URL}/api/v1/distance/calculate" \
  -H "Content-Type: application/json" \
  -d '{
    "origin": {
      "type": "coordinates",
      "coordinates": {
        "latitude": 6.5244,
        "longitude": 3.3792
      }
    },
    "destination": {
      "type": "coordinates", 
      "coordinates": {
        "latitude": 6.6018,
        "longitude": 3.3515
      }
    },
    "transport_mode": "car"
  }' | jq . | head -20

echo -e "\n\nSwagger UI should be available at: ${BASE_URL}/api/docs"
echo "OpenAPI JSON should be available at: ${BASE_URL}/api/openapi.json"
