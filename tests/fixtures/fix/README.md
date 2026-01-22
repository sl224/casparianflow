# FIX Protocol Test Fixtures

These fixtures are synthetic FIX messages for testing the Casparian FIX parser.

## Files

| File | Description | Message Types |
|------|-------------|---------------|
| `order_lifecycle.fix` | Order flow scenarios | D, 8, F, G, 9, H |
| `session_events.fix` | Session management messages | A, 0, 1, 2, 3, 4, 5, j |
| `mixed_messages.fix` | Combined order and session messages | Mixed |
| `prefixed_logs.fix` | Prefixed and multi-message logs | Mixed |
| `decimal_rounding.fix` | High-precision decimal rounding | D |
| `soh_delimiter.fix` | SOH (\x01) delimited messages | D, 8 |

## Format

Messages use pipe (`|`) delimiter unless noted otherwise. All messages follow FIX 4.4 format.

## Attribution

Fixture format based on publicly available FIX message samples:
- [FixSim Sample Messages](https://www.fixsim.com/sample-fix-messages)
- [Jettek FIX Analysis Examples](https://jettekfix.com/2019/10/07/analyzing-fix-log-files/)

## Usage

```bash
# Run FIX parser with fixtures
FIX_TZ=UTC ./target/release/casparian run parsers/fix/fix_parser.py tests/fixtures/fix/order_lifecycle.fix
```
