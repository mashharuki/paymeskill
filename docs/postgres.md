# Postgres Schema (MVP)

Run migrations automatically at backend startup, or apply this SQL manually:

```sql
create table if not exists users (
  id uuid primary key,
  email text not null,
  region text not null,
  roles text[] not null,
  tools_used text[] not null,
  attributes jsonb not null,
  created_at timestamptz not null default now()
);

create table if not exists sponsored_apis (
  id uuid primary key,
  name text not null,
  sponsor text not null,
  description text,
  upstream_url text not null,
  upstream_method text not null,
  upstream_headers jsonb not null default '{}'::jsonb,
  price_cents bigint not null,
  budget_total_cents bigint not null,
  budget_remaining_cents bigint not null,
  active boolean not null default true,
  service_key text not null unique,
  created_at timestamptz not null default now()
);

create table if not exists sponsored_api_calls (
  id uuid primary key,
  sponsored_api_id uuid not null references sponsored_apis(id) on delete cascade,
  payment_mode text not null,
  amount_cents bigint not null,
  tx_hash text,
  caller text,
  created_at timestamptz not null default now()
);

create index if not exists sponsored_api_calls_api_id_idx
  on sponsored_api_calls(sponsored_api_id);
```

Environment:

- `DATABASE_URL=postgres://postgres:postgres@localhost:5432/payloadexchange`
