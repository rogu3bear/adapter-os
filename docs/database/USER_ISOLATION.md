# Database User Isolation

## Overview

Production PostgreSQL deployments **must** use separate database users for application operations and administrative tasks. This provides defense-in-depth security by limiting the scope of potential credential compromise.

## Required Users

### `adapteros_app` - Application User

**Purpose:** Read/write operations for normal application functionality.

**Permissions:**
- `SELECT`, `INSERT`, `UPDATE`, `DELETE` on all tables in `public` schema
- No schema modification privileges
- No migration execution privileges

**Usage:** Used by the application runtime for all normal database operations.

### `adapteros_admin` - Administrative User

**Purpose:** Database migrations, schema changes, and maintenance operations.

**Permissions:**
- Full database privileges (for migrations)
- Can create/modify tables, indexes, and other schema objects
- Used only during deployment and maintenance

**Usage:** Used by migration scripts and administrative tools.

## Setup Instructions

### 1. Create Users

```sql
-- Create application user
CREATE USER adapteros_app WITH PASSWORD 'your-secure-password-here';

-- Create admin user
CREATE USER adapteros_admin WITH PASSWORD 'your-secure-admin-password-here';
```

### 2. Grant Permissions

```sql
-- Grant application user read/write access to all tables
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO adapteros_app;

-- Grant application user access to sequences (for auto-increment columns)
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO adapteros_app;

-- Grant admin user full database privileges
GRANT ALL PRIVILEGES ON DATABASE adapteros TO adapteros_admin;
GRANT ALL PRIVILEGES ON SCHEMA public TO adapteros_admin;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO adapteros_admin;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO adapteros_admin;

-- Ensure admin user can create new objects
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO adapteros_admin;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO adapteros_admin;
```

### 3. Configure Connection Strings

**Application Connection String:**
```bash
DATABASE_URL=postgresql://adapteros_app:password@localhost:5432/adapteros
```

**Admin Connection String (for migrations):**
```bash
DATABASE_URL=postgresql://adapteros_admin:admin-password@localhost:5432/adapteros
```

## Best Practices

1. **Never use admin credentials in application runtime** - Only use `adapteros_app` for normal operations
2. **Rotate passwords regularly** - Change passwords on a regular schedule
3. **Use strong passwords** - Minimum 32 characters, mix of alphanumeric and special characters
4. **Store credentials securely** - Use environment variables or secret management systems, never commit to version control
5. **Limit admin access** - Only grant `adapteros_admin` access to deployment systems and CI/CD pipelines

## Development vs Production

**Development:** Single user is acceptable for local development.

**Production:** **Must** use separate users as described above.

## Future Enhancements

- Automated setup script for user creation
- CI/CD integration for credential rotation
- Connection string validation to enforce user separation

## References

- PostgreSQL User Management: https://www.postgresql.org/docs/current/user-manag.html
- PostgreSQL Privileges: https://www.postgresql.org/docs/current/ddl-priv.html

