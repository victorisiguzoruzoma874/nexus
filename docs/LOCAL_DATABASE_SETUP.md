# Local PostgreSQL Database Setup

This guide will help you set up a local PostgreSQL database for development and testing.

## Step 1: Verify PostgreSQL Installation

Check if PostgreSQL is installed and running:

```bash
# Check PostgreSQL version
psql --version

# Check if PostgreSQL service is running
sudo systemctl status postgresql

# If not running, start it
sudo systemctl start postgresql

# Enable auto-start on boot (optional)
sudo systemctl enable postgresql
```

## Step 2: Access PostgreSQL

```bash
# Switch to postgres user
sudo -i -u postgres

# Access PostgreSQL prompt
psql
```

You should see a prompt like: `postgres=#`

## Step 3: Create Database and User

Run these commands in the PostgreSQL prompt:

```sql
-- Create a new database
CREATE DATABASE nexuscare;

-- Create a user with password (change 'your_password' to something secure)
CREATE USER nexuscare_user WITH PASSWORD 'your_password';

-- Grant all privileges on the database to the user
GRANT ALL PRIVILEGES ON DATABASE nexuscare TO nexuscare_user;

-- Connect to the nexuscare database
\c nexuscare

-- Grant schema privileges (PostgreSQL 15+)
GRANT ALL ON SCHEMA public TO nexuscare_user;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO nexuscare_user;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO nexuscare_user;

-- Set default privileges for future tables
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO nexuscare_user;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO nexuscare_user;

-- Exit PostgreSQL
\q
```

Exit from postgres user:
```bash
exit
```

## Step 4: Test Connection

Test if you can connect with the new user:

```bash
psql -U nexuscare_user -d nexuscare -h localhost
# Enter password when prompted
```

If successful, you'll see: `nexuscare=>`

Exit with `\q`

## Step 5: Create .env File

Create a `.env` file in your project root:

```bash
cp .env.example .env
```

Edit `.env` and update the database URL:

```env
# Database Configuration
DATABASE_URL=postgresql://nexuscare_user:your_password@localhost:5432/nexuscare
DATABASE_MAX_CONNECTIONS=10
DATABASE_TIMEOUT_SECONDS=30

# Generate these keys
JWT_SECRET=your_jwt_secret_here
ENCRYPTION_KEY=your_encryption_key_here

# Server
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
API_BASE_URL=http://localhost:8080

# For testing, you can use mock values for external services
PAYSTACK_SECRET_KEY=sk_test_mock_key_for_testing
PAYSTACK_PUBLIC_KEY=pk_test_mock_key_for_testing
PAYSTACK_API_URL=https://api.paystack.co

# Geocoding (no key needed)
GEOCODING_API_URL=https://nominatim.openstreetmap.org
GEOCODING_USER_AGENT=NexusCare/1.0 (test@nexuscare.com)

# Email (for testing, logs will be printed to console)
SMTP_HOST=smtp.gmail.com
SMTP_PORT=587
SMTP_USERNAME=test@example.com
SMTP_PASSWORD=test_password
SMTP_FROM_EMAIL=noreply@nexuscare.com
SMTP_USE_TLS=true

# Logging
RUST_LOG=nexuscare_backend=debug,tower_http=debug,sqlx=info
LOG_FORMAT=pretty

# Application
APP_ENV=development
ENABLE_API_DOCS=true
DEFAULT_SERVICE_RADIUS_KM=5.0

# CORS
CORS_ALLOWED_ORIGINS=http://localhost:3000,http://localhost:5173

# Security
BCRYPT_COST=12
```

## Step 6: Generate Encryption Keys

Generate secure keys for JWT and encryption:

```bash
# Generate JWT secret (64 bytes)
echo "JWT_SECRET=$(openssl rand -base64 64)"

# Generate encryption key (32 bytes for AES-256)
echo "ENCRYPTION_KEY=$(openssl rand -base64 32)"

# Generate salt (optional)
echo "ENCRYPTION_SALT=$(openssl rand -base64 32)"
```

Copy these values into your `.env` file.

## Step 7: Install SQLx CLI

Install the SQLx command-line tool for running migrations:

```bash
cargo install sqlx-cli --no-default-features --features postgres
```

## Step 8: Run Database Migrations

Run the migrations to create all tables:

```bash
# Make sure you're in the project directory
cd ~/Desktop/Nexus_care/nexus

# Run migrations
sqlx migrate run
```

You should see output like:
```
Applied 20240001_create_enums.sql
Applied 20240002_create_hospitals.sql
Applied 20240003_create_users.sql
...
```

## Step 9: Verify Database Setup

Check that tables were created:

```bash
psql -U nexuscare_user -d nexuscare -h localhost
```

In the PostgreSQL prompt:

```sql
-- List all tables
\dt

-- You should see tables like:
-- hospitals, users, hospital_locations, hospital_payment_methods, etc.

-- Check a specific table structure
\d hospitals

-- Exit
\q
```

## Step 10: Test the Application

Now you can run the application:

```bash
cargo run
```

You should see:
```
🚀 Server listening on http://0.0.0.0:8080
📚 API Documentation: http://localhost:8080/api/docs
```

## Quick Commands Reference

### PostgreSQL Service Management
```bash
# Start PostgreSQL
sudo systemctl start postgresql

# Stop PostgreSQL
sudo systemctl stop postgresql

# Restart PostgreSQL
sudo systemctl restart postgresql

# Check status
sudo systemctl status postgresql
```

### Database Management
```bash
# Connect to database
psql -U nexuscare_user -d nexuscare -h localhost

# List databases
psql -U postgres -l

# Drop database (careful!)
psql -U postgres -c "DROP DATABASE nexuscare;"

# Recreate database
psql -U postgres -c "CREATE DATABASE nexuscare;"
```

### Migration Management
```bash
# Run all pending migrations
sqlx migrate run

# Revert last migration
sqlx migrate revert

# Create new migration
sqlx migrate add migration_name

# Check migration status
sqlx migrate info
```

### Useful PostgreSQL Commands (in psql)
```sql
-- List all databases
\l

-- List all tables
\dt

-- Describe table structure
\d table_name

-- List all users
\du

-- Switch database
\c database_name

-- Show current database
SELECT current_database();

-- Show all tables with row counts
SELECT schemaname,relname,n_live_tup 
FROM pg_stat_user_tables 
ORDER BY n_live_tup DESC;

-- Exit
\q
```

## Troubleshooting

### Issue: "peer authentication failed"

**Solution:** Edit PostgreSQL config to allow password authentication:

```bash
# Find the pg_hba.conf file
sudo find /etc/postgresql -name pg_hba.conf

# Edit it (usually at /etc/postgresql/14/main/pg_hba.conf)
sudo nano /etc/postgresql/14/main/pg_hba.conf

# Change this line:
# local   all             all                                     peer

# To:
local   all             all                                     md5

# Restart PostgreSQL
sudo systemctl restart postgresql
```

### Issue: "database does not exist"

**Solution:** Create the database:

```bash
sudo -u postgres createdb nexuscare
```

### Issue: "role does not exist"

**Solution:** Create the user:

```bash
sudo -u postgres psql -c "CREATE USER nexuscare_user WITH PASSWORD 'your_password';"
```

### Issue: "permission denied for schema public"

**Solution:** Grant permissions:

```bash
sudo -u postgres psql -d nexuscare -c "GRANT ALL ON SCHEMA public TO nexuscare_user;"
```

### Issue: "connection refused"

**Solution:** Check if PostgreSQL is running:

```bash
sudo systemctl status postgresql
sudo systemctl start postgresql
```

### Issue: Migration fails

**Solution:** Check migration status and revert if needed:

```bash
sqlx migrate info
sqlx migrate revert  # Revert last migration
sqlx migrate run     # Try again
```

## Testing Your Setup

### 1. Test Database Connection

```bash
# Test connection
psql -U nexuscare_user -d nexuscare -h localhost -c "SELECT version();"
```

### 2. Test Application

```bash
# Run the application
cargo run

# In another terminal, test the health endpoint
curl http://localhost:8080/health
```

### 3. Test API Documentation

Open your browser:
- Swagger UI: http://localhost:8080/api/docs
- Health Check: http://localhost:8080/health

## Resetting the Database

If you need to start fresh:

```bash
# Drop all tables (careful!)
psql -U nexuscare_user -d nexuscare -h localhost -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"

# Grant permissions again
psql -U postgres -d nexuscare -c "GRANT ALL ON SCHEMA public TO nexuscare_user;"

# Run migrations again
sqlx migrate run
```

## Moving to Production (Render)

When you're ready to deploy to Render:

1. **Create database on Render:**
   - Go to Render Dashboard
   - Create a new PostgreSQL database
   - Copy the connection string

2. **Update production .env:**
   ```env
   DATABASE_URL=postgres://user:password@host.render.com:5432/database
   ```

3. **Run migrations on production:**
   ```bash
   # Set DATABASE_URL temporarily
   export DATABASE_URL=postgres://user:password@host.render.com:5432/database
   
   # Run migrations
   sqlx migrate run
   ```

4. **Deploy your application:**
   - Push to GitHub
   - Connect Render to your repository
   - Set environment variables in Render dashboard
   - Deploy!

## Next Steps

1. ✅ Local database is set up
2. ✅ Migrations are run
3. ✅ Application can connect to database
4. 🎯 Test the API endpoints
5. 🎯 Develop and test locally
6. 🎯 Deploy to Render when ready

---

**You're all set! Your local PostgreSQL database is ready for development and testing.** 🚀
