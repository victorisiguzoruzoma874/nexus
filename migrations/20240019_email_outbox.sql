-- Email outbox for async delivery

CREATE TABLE email_outbox (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    to_email     VARCHAR(255) NOT NULL,
    subject      VARCHAR(255) NOT NULL,
    text_body    TEXT        NOT NULL,
    html_body    TEXT,
    status       VARCHAR(20) NOT NULL DEFAULT 'pending',
    attempts     INTEGER     NOT NULL DEFAULT 0,
    last_error   TEXT,
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sent_at      TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_email_outbox_status_scheduled
    ON email_outbox (status, scheduled_at);

CREATE INDEX idx_email_outbox_created_at
    ON email_outbox (created_at);

CREATE TRIGGER trg_email_outbox_updated_at
    BEFORE UPDATE ON email_outbox
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
