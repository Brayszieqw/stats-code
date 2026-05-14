# Synthetic Dataset Provenance

## Generation

All files in this directory are **fully synthetic** (no real patient data).

| File | N | Generator Script |
| --- | --- | --- |
| `small_n40.csv` | 40 | `datasets/_preprocess/gen_synthetic.py` |
| `medium_n200.csv` | 200 | `datasets/_preprocess/gen_synthetic.py` |
| `large_n2000.csv` | 2000 | `datasets/_preprocess/gen_synthetic.py` |

**Random seed:** `20260510` (fixed for reproducibility)

## License

Synthetic / public domain. No restrictions on use or redistribution.

## Data Generation Model

```
age      ~ Uniform(30, 80)
bmi      ~ Uniform(18, 40)
linear_y = 4 + 0.42*age + 0.85*bmi + Normal(0, 2.5)
logit(p) = -5.2 + 0.055*age + 0.075*bmi
disease  ~ Bernoulli(p)
hazard   = exp(-4 + 0.03*age + 0.02*bmi)
time_raw ~ Exponential(1/hazard)
censor   ~ Uniform(2, 20)
time     = min(time_raw, censor)
death    = 1{time_raw <= censor}
group    ~ Uniform({0, 1})
```

## Columns

| Column | Type | Description |
| --- | --- | --- |
| `age` | float | Age in years (30–80) |
| `bmi` | float | Body mass index (18–40) |
| `linear_y` | float | Continuous outcome for linear regression |
| `disease` | int (0/1) | Binary outcome for logistic regression |
| `time` | float | Follow-up time for survival analysis |
| `death` | int (0/1) | Event indicator (1 = event occurred) |
| `group` | int (0/1) | Binary group variable for Table One |

## Applicable Methods

`linear`, `logistic`, `cox`, `survival`, `tableone`

## Modification History

| Date | Change |
| --- | --- |
| 2026-05-13 | Initial generation (seed 20260510) |
