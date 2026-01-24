import { test, expect } from '@playwright/test'

test.describe('Query Console Screen', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/query')
  })

  test('should display page header with title and subtitle', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Query Console' })).toBeVisible()
    await expect(page.getByText('Run SQL queries on output data')).toBeVisible()
  })

  test('should display SQL editor', async ({ page }) => {
    const sqlEditor = page.getByTestId('sql-editor')
    await expect(sqlEditor).toBeVisible()

    // Check it contains SQL
    await expect(sqlEditor).toHaveValue(/SELECT/)
    await expect(sqlEditor).toHaveValue(/FROM cf_api_jobs/)
  })

  test('should have run query button', async ({ page }) => {
    const runBtn = page.getByTestId('run-query-btn')
    await expect(runBtn).toBeVisible()
    await expect(runBtn).toBeEnabled()
    await expect(runBtn).toContainText('Run Query')
  })

  test('should display results metadata after running query', async ({ page }) => {
    // Click run query to get results
    await page.getByTestId('run-query-btn').click()

    // Wait for results to appear (mock data returns quickly)
    await expect(page.getByTestId('row-count')).toBeVisible()
    await expect(page.getByTestId('row-count')).toHaveText('247 rows')
    await expect(page.getByTestId('exec-time')).toHaveText('23ms')
  })

  test('should display results table with column headers after running query', async ({ page }) => {
    // Click run query to get results
    await page.getByTestId('run-query-btn').click()

    const resultsTable = page.getByTestId('results-table')
    await expect(resultsTable).toBeVisible()

    // Check column headers (use locator within table-header class to avoid SQL editor match)
    const tableHeader = page.locator('.table-header')
    await expect(tableHeader.getByText('output_name')).toBeVisible()
    await expect(tableHeader.getByText('row_count')).toBeVisible()
    await expect(tableHeader.getByText('total_bytes')).toBeVisible()
  })

  test('should display result rows after running query', async ({ page }) => {
    // Click run query to get results
    await page.getByTestId('run-query-btn').click()

    await expect(page.getByTestId('result-row-0')).toBeVisible()
    await expect(page.getByTestId('result-row-1')).toBeVisible()
    await expect(page.getByTestId('result-row-2')).toBeVisible()
    await expect(page.getByTestId('result-row-3')).toBeVisible()
    await expect(page.getByTestId('result-row-4')).toBeVisible()
  })

  test('should display correct data in result rows after running query', async ({ page }) => {
    // Click run query to get results
    await page.getByTestId('run-query-btn').click()

    const firstRow = page.getByTestId('result-row-0')
    await expect(firstRow).toBeVisible()

    await expect(firstRow.getByText('fix_order_lifecycle')).toBeVisible()
    await expect(firstRow.getByText('1,247,832')).toBeVisible()
    await expect(firstRow.getByText('2.4 GB')).toBeVisible()
  })

  test('SQL editor should be editable', async ({ page }) => {
    const sqlEditor = page.getByTestId('sql-editor')

    // Clear and type new SQL
    await sqlEditor.fill('SELECT * FROM users LIMIT 5;')

    await expect(sqlEditor).toHaveValue('SELECT * FROM users LIMIT 5;')
  })

  test('should maintain SQL editor content after typing', async ({ page }) => {
    const sqlEditor = page.getByTestId('sql-editor')
    const originalValue = await sqlEditor.inputValue()

    // Add to the SQL
    await sqlEditor.focus()
    await sqlEditor.press('End')
    await sqlEditor.type(' -- added comment')

    const newValue = await sqlEditor.inputValue()
    expect(newValue).toContain('-- added comment')
  })

  test('should show placeholder text before running query', async ({ page }) => {
    // Before running query, should show placeholder
    await expect(page.getByText('Run a query to see results')).toBeVisible()
  })
})
