-- M2 local policy only; immutable allowances, durable one-shot intents and query receipts.
CREATE TABLE aw_budget_accounts (
 id TEXT PRIMARY KEY, descriptor TEXT NOT NULL, capacity INTEGER NOT NULL CHECK(capacity>=0),
 reserved INTEGER NOT NULL DEFAULT 0 CHECK(reserved>=0), spent INTEGER NOT NULL DEFAULT 0 CHECK(spent>=0),
 CHECK(reserved<=capacity AND spent<=capacity-reserved)
) STRICT;
CREATE TABLE aw_budget_intents (
 id TEXT PRIMARY KEY, account TEXT NOT NULL REFERENCES aw_budget_accounts(id), descriptor TEXT NOT NULL,
 reservation INTEGER NOT NULL CHECK(reservation>=0),
 state TEXT NOT NULL CHECK(state IN ('PREPARING','ATTEMPTING','UNKNOWN','CONFIRMED','RELEASED')),
 cancelled INTEGER NOT NULL DEFAULT 0 CHECK(cancelled IN (0,1)), charge INTEGER CHECK(charge>=0)
) STRICT;
CREATE TABLE aw_budget_receipts (
 call_id TEXT NOT NULL REFERENCES aw_budget_intents(id), id TEXT NOT NULL, descriptor TEXT NOT NULL,
 PRIMARY KEY(call_id,id)
) STRICT;
