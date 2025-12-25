"""
Slow Processor Plugin - E2E Demo

Simulates a computationally expensive process by:
1. Reading input data in chunks
2. Sleeping between chunks to simulate processing time
3. Yielding Arrow batches incrementally

Watch the UI metrics update in real-time!
"""

import time
import pandas as pd
import pyarrow as pa

# Plugin manifest - defines routing and output
MANIFEST = {
    "pattern": "demo/data/*.csv",
    "topic": "processed_output",
}


class Handler:
    """
    Demo plugin that processes files slowly for UI testing.

    Uses the standard execute() pattern that yields Arrow batches.
    """

    def execute(self, file_path: str):
        """
        Process file in batches with artificial delays.

        Args:
            file_path: Path to input file

        Yields:
            Arrow batches of processed data
        """
        batch_size = 5  # Small batches for demo visibility
        delay_seconds = 1.5  # Delay between batches

        print(f"[slow_processor] Starting: {file_path}")

        # Read input CSV
        df = pd.read_csv(file_path)
        total_rows = len(df)

        print(f"[slow_processor] Total rows: {total_rows}, batch_size: {batch_size}")

        batch_number = 0
        start_time = time.time()

        # Process in batches with delays
        for i in range(0, total_rows, batch_size):
            batch_number += 1
            batch_df = df.iloc[i:i + batch_size].copy()

            # Simulate expensive computation
            print(f"[slow_processor] Processing batch {batch_number}...")
            time.sleep(delay_seconds)

            # Add processing metadata
            batch_df["_batch"] = batch_number
            batch_df["_processed_at"] = pd.Timestamp.now().isoformat()

            # Yield as Arrow table
            yield pa.Table.from_pandas(batch_df)

            elapsed = time.time() - start_time
            processed = min((batch_number * batch_size), total_rows)
            print(f"[slow_processor] Batch {batch_number} done. "
                  f"Progress: {processed}/{total_rows} ({100*processed/total_rows:.0f}%) "
                  f"Elapsed: {elapsed:.1f}s")

        total_time = time.time() - start_time
        print(f"[slow_processor] Complete! {total_rows} rows in {total_time:.1f}s")
