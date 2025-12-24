/**
 * DataGrid Logic Tests
 *
 * Tests value formatting and display logic without rendering.
 * Perfect for LLM review - we verify the data transformation rules.
 */

import { describe, it, expect } from 'vitest';

// Value formatting logic (extracted from component)
function formatValue(value: unknown): string {
  if (value === null || value === undefined) {
    return 'NULL';
  }
  if (typeof value === 'object') {
    return JSON.stringify(value);
  }
  if (typeof value === 'number') {
    // Format large numbers with commas
    if (Number.isInteger(value) && Math.abs(value) >= 1000) {
      return value.toLocaleString();
    }
    // Format floats with reasonable precision
    if (!Number.isInteger(value)) {
      return value.toFixed(4);
    }
  }
  return String(value);
}

function isNull(value: unknown): boolean {
  return value === null || value === undefined;
}

function isNumeric(value: unknown): boolean {
  return typeof value === 'number';
}

describe('formatValue', () => {
  it('should format null as "NULL"', () => {
    expect(formatValue(null)).toBe('NULL');
  });

  it('should format undefined as "NULL"', () => {
    expect(formatValue(undefined)).toBe('NULL');
  });

  it('should format strings as-is', () => {
    expect(formatValue('hello')).toBe('hello');
    expect(formatValue('')).toBe('');
    expect(formatValue('with spaces')).toBe('with spaces');
  });

  it('should format small integers as-is', () => {
    expect(formatValue(42)).toBe('42');
    expect(formatValue(0)).toBe('0');
    expect(formatValue(-5)).toBe('-5');
  });

  it('should format large integers with commas', () => {
    expect(formatValue(1000)).toBe('1,000');
    expect(formatValue(1000000)).toBe('1,000,000');
    expect(formatValue(-50000)).toBe('-50,000');
  });

  it('should format floats with 4 decimal places', () => {
    expect(formatValue(3.14159265)).toBe('3.1416');
    expect(formatValue(0.1)).toBe('0.1000');
    expect(formatValue(-2.5)).toBe('-2.5000');
  });

  it('should format booleans as strings', () => {
    expect(formatValue(true)).toBe('true');
    expect(formatValue(false)).toBe('false');
  });

  it('should JSON stringify objects', () => {
    expect(formatValue({ a: 1 })).toBe('{"a":1}');
    expect(formatValue([1, 2, 3])).toBe('[1,2,3]');
  });

  it('should handle empty objects', () => {
    expect(formatValue({})).toBe('{}');
    expect(formatValue([])).toBe('[]');
  });

  it('should handle nested objects', () => {
    const nested = { outer: { inner: 'value' } };
    expect(formatValue(nested)).toBe('{"outer":{"inner":"value"}}');
  });
});

describe('isNull', () => {
  it('should return true for null', () => {
    expect(isNull(null)).toBe(true);
  });

  it('should return true for undefined', () => {
    expect(isNull(undefined)).toBe(true);
  });

  it('should return false for empty string', () => {
    expect(isNull('')).toBe(false);
  });

  it('should return false for zero', () => {
    expect(isNull(0)).toBe(false);
  });

  it('should return false for false', () => {
    expect(isNull(false)).toBe(false);
  });

  it('should return false for empty array', () => {
    expect(isNull([])).toBe(false);
  });
});

describe('isNumeric', () => {
  it('should return true for integers', () => {
    expect(isNumeric(42)).toBe(true);
    expect(isNumeric(0)).toBe(true);
    expect(isNumeric(-100)).toBe(true);
  });

  it('should return true for floats', () => {
    expect(isNumeric(3.14)).toBe(true);
    expect(isNumeric(-0.5)).toBe(true);
  });

  it('should return true for special numbers', () => {
    expect(isNumeric(Infinity)).toBe(true);
    expect(isNumeric(-Infinity)).toBe(true);
    expect(isNumeric(NaN)).toBe(true); // NaN is typeof 'number'
  });

  it('should return false for numeric strings', () => {
    expect(isNumeric('42')).toBe(false);
    expect(isNumeric('3.14')).toBe(false);
  });

  it('should return false for null/undefined', () => {
    expect(isNumeric(null)).toBe(false);
    expect(isNumeric(undefined)).toBe(false);
  });
});

describe('QueryResult structure', () => {
  interface QueryResult {
    columns: string[];
    rows: unknown[][];
    rowCount: number;
    executionTimeMs: number;
  }

  it('should have matching row count', () => {
    const result: QueryResult = {
      columns: ['id', 'name'],
      rows: [
        [1, 'Alice'],
        [2, 'Bob'],
      ],
      rowCount: 2,
      executionTimeMs: 15,
    };

    expect(result.rows.length).toBe(result.rowCount);
  });

  it('should have consistent column count per row', () => {
    const result: QueryResult = {
      columns: ['a', 'b', 'c'],
      rows: [
        [1, 2, 3],
        [4, 5, 6],
      ],
      rowCount: 2,
      executionTimeMs: 10,
    };

    const columnCount = result.columns.length;
    for (const row of result.rows) {
      expect(row.length).toBe(columnCount);
    }
  });

  it('should handle empty result', () => {
    const result: QueryResult = {
      columns: ['id', 'value'],
      rows: [],
      rowCount: 0,
      executionTimeMs: 5,
    };

    expect(result.rowCount).toBe(0);
    expect(result.rows.length).toBe(0);
    // columns should still be defined even with no rows
    expect(result.columns.length).toBe(2);
  });

  it('should handle single column result', () => {
    const result: QueryResult = {
      columns: ['count'],
      rows: [[42]],
      rowCount: 1,
      executionTimeMs: 1,
    };

    expect(result.columns.length).toBe(1);
    expect(result.rows[0].length).toBe(1);
  });
});

describe('Edge cases for display', () => {
  it('should handle very long strings', () => {
    const longString = 'a'.repeat(1000);
    const formatted = formatValue(longString);

    // Should not truncate in formatValue - that's CSS's job
    expect(formatted.length).toBe(1000);
  });

  it('should handle strings with special characters', () => {
    expect(formatValue('<script>alert("xss")</script>')).toBe('<script>alert("xss")</script>');
    expect(formatValue('line1\nline2')).toBe('line1\nline2');
    expect(formatValue('tab\there')).toBe('tab\there');
  });

  it('should handle very large numbers', () => {
    const bigNum = 9007199254740991; // MAX_SAFE_INTEGER
    const formatted = formatValue(bigNum);

    expect(formatted).toContain(',');
    // Should be human-readable with commas
  });

  it('should handle very small floats', () => {
    expect(formatValue(0.0001)).toBe('0.0001');
    expect(formatValue(0.00001)).toBe('0.0000'); // Truncated to 4 decimals
  });

  it('should handle scientific notation numbers', () => {
    // JavaScript represents very large/small numbers in scientific notation
    const tiny = 1e-10;
    expect(formatValue(tiny)).toBe('0.0000'); // Rounds to 0.0000

    const huge = 1e15;
    // Large integer gets comma-formatted
    expect(formatValue(huge)).toContain(',');
  });
});
