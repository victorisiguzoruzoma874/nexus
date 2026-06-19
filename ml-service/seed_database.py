"""
Seed PostgreSQL patient_training_data table from the synthetic CSV,
and export the table back to CSV when needed.

Usage:
  python seed_database.py --seed          # CSV → PostgreSQL
  python seed_database.py --export        # PostgreSQL → CSV
  python seed_database.py --seed --export # both
"""
import os
import argparse
import pandas as pd
from sqlalchemy import create_engine, text
from dotenv import load_dotenv

load_dotenv()

CSV_PATH = "data/patients_training.csv"
EXPORT_PATH = "data/patients_export.csv"


def get_engine():
    db_url = os.environ.get("DATABASE_URL", "")
    if not db_url:
        raise RuntimeError("DATABASE_URL not set in .env")
    return create_engine(db_url.replace("postgres://", "postgresql://", 1))


def seed(engine):
    if not os.path.exists(CSV_PATH):
        raise FileNotFoundError(f"{CSV_PATH} not found — run generate_training_data.py first")

    df = pd.read_csv(CSV_PATH)
    for col in ["smoking_status", "alcohol_consumption"]:
        df[col] = df[col].map({"True": True, "False": False, True: True, False: False}).fillna(False)

    # Keep only columns the table expects
    cols = [
        "patient_id", "gender", "age", "blood_group", "genotype",
        "height_cm", "weight_kg", "disease_type", "symptoms",
        "existing_conditions", "severity_level", "severity_ordinal",
        "weather_condition", "smoking_status", "alcohol_consumption",
        "exercise_habits", "diet_type", "water_source", "patient_category",
        "state", "occupation", "predictive_risk_score", "mortality_risk",
        "readmission_prediction", "drug_recommendation", "was_readmitted",
    ]
    df = df[cols].copy()
    df["source"] = "synthetic"

    with engine.begin() as conn:
        # Truncate first so re-runs are idempotent
        conn.execute(text("TRUNCATE patient_training_data RESTART IDENTITY"))

    df.to_sql("patient_training_data", engine, if_exists="append", index=False, method="multi", chunksize=500)
    print(f"✓ Seeded {len(df)} rows into patient_training_data")


def export(engine):
    os.makedirs("data", exist_ok=True)
    query = text("""
        SELECT
            patient_id, gender, age, blood_group, genotype,
            height_cm, weight_kg, disease_type, symptoms,
            existing_conditions, severity_level, severity_ordinal,
            weather_condition, smoking_status::int AS smoking_status,
            alcohol_consumption::int AS alcohol_consumption,
            exercise_habits, diet_type, water_source, patient_category,
            state, occupation, predictive_risk_score, mortality_risk,
            readmission_prediction, drug_recommendation, was_readmitted, source
        FROM patient_training_data
        ORDER BY id
    """)
    with engine.connect() as conn:
        df = pd.read_sql(query, conn)
    df.to_csv(EXPORT_PATH, index=False)
    print(f"✓ Exported {len(df)} rows → {EXPORT_PATH}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed",   action="store_true", help="Load CSV into PostgreSQL")
    parser.add_argument("--export", action="store_true", help="Export PostgreSQL table to CSV")
    args = parser.parse_args()

    if not args.seed and not args.export:
        parser.print_help()
    else:
        engine = get_engine()
        if args.seed:
            seed(engine)
        if args.export:
            export(engine)
