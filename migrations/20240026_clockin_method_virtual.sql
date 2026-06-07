-- Add `virtual` to the clockin_method enum so virtual shift clock-ins can be
-- distinguished from in-person GPS / QR / manual methods (FRS §3.6.5).
ALTER TYPE clockin_method ADD VALUE IF NOT EXISTS 'virtual';
