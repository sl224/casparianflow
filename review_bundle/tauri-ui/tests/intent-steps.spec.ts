import { test, expect } from '@playwright/test'

test.describe('Selection Step', () => {
  test.beforeEach(async ({ page }) => {
    // Navigate to a session and ensure we're on the selection step
    await page.goto('/sessions/a1b2c3d4-e5f6-7890-abcd-ef1234567890')
    // Click on selection step to make sure we're there
    await page.getByTestId('step-selection').click()
  })

  test('should display selection step content', async ({ page }) => {
    await expect(page.getByTestId('selection-step')).toBeVisible()
  })

  test('should display intent input textarea', async ({ page }) => {
    const intentInput = page.getByTestId('intent-input')
    await expect(intentInput).toBeVisible()
  })

  test('should display root directory input', async ({ page }) => {
    const rootDirInput = page.getByTestId('root-dir-input')
    await expect(rootDirInput).toBeVisible()
    await expect(rootDirInput).toHaveValue('/data/sales')
  })

  test('should display browse button', async ({ page }) => {
    await expect(page.getByTestId('browse-btn')).toBeVisible()
  })

  test('should display scan button', async ({ page }) => {
    const scanBtn = page.getByTestId('scan-btn')
    await expect(scanBtn).toBeVisible()
    await expect(scanBtn).toBeEnabled()
  })

  test('should display confidence badge', async ({ page }) => {
    const confidenceBadge = page.getByTestId('confidence-badge')
    await expect(confidenceBadge).toBeVisible()
    await expect(confidenceBadge).toContainText('confidence')
  })

  test('should display sample files list', async ({ page }) => {
    const sampleFiles = page.getByTestId('sample-files')
    await expect(sampleFiles).toBeVisible()
  })

  test('should display extension checkboxes', async ({ page }) => {
    // Check for extension checkboxes in the proposal
    await expect(page.getByTestId('ext-checkbox-.csv')).toBeVisible()
  })

  test('should display approve button', async ({ page }) => {
    const approveBtn = page.getByTestId('approve-btn')
    await expect(approveBtn).toBeVisible()
    await expect(approveBtn).toBeEnabled()
  })

  test('should display modify button', async ({ page }) => {
    await expect(page.getByTestId('modify-btn')).toBeVisible()
  })

  test('should allow editing intent text', async ({ page }) => {
    const intentInput = page.getByTestId('intent-input')
    await intentInput.fill('Process all Q4 sales data')
    await expect(intentInput).toHaveValue('Process all Q4 sales data')
  })

  test('should allow editing root directory', async ({ page }) => {
    const rootDirInput = page.getByTestId('root-dir-input')
    await rootDirInput.fill('/new/path/to/data')
    await expect(rootDirInput).toHaveValue('/new/path/to/data')
  })
})

test.describe('Tag Rules Step', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/sessions/a1b2c3d4-e5f6-7890-abcd-ef1234567890')
    await page.getByTestId('step-tags').click()
  })

  test('should display tag rules step content', async ({ page }) => {
    await expect(page.getByTestId('tag-rules-step')).toBeVisible()
  })

  test('should display rules list', async ({ page }) => {
    const rulesList = page.getByTestId('rules-list')
    await expect(rulesList).toBeVisible()
  })

  test('should display rule cards', async ({ page }) => {
    await expect(page.getByTestId('rule-rule-1')).toBeVisible()
    await expect(page.getByTestId('rule-rule-2')).toBeVisible()
  })

  test('should display add custom rule button', async ({ page }) => {
    await expect(page.getByTestId('add-rule-btn')).toBeVisible()
  })

  test('should display apply rules button', async ({ page }) => {
    const applyBtn = page.getByTestId('apply-rules-btn')
    await expect(applyBtn).toBeVisible()
    await expect(applyBtn).toBeEnabled()
  })
})

test.describe('Path Fields Step', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/sessions/a1b2c3d4-e5f6-7890-abcd-ef1234567890')
    await page.getByTestId('step-pathfields').click()
  })

  test('should display path fields step content', async ({ page }) => {
    await expect(page.getByTestId('path-fields-step')).toBeVisible()
  })

  test('should display fields list', async ({ page }) => {
    const fieldsList = page.getByTestId('fields-list')
    await expect(fieldsList).toBeVisible()
  })

  test('should display field cards', async ({ page }) => {
    await expect(page.getByTestId('field-year')).toBeVisible()
    await expect(page.getByTestId('field-month')).toBeVisible()
    await expect(page.getByTestId('field-quarter')).toBeVisible()
  })

  test('should display add custom field button', async ({ page }) => {
    await expect(page.getByTestId('add-field-btn')).toBeVisible()
  })

  test('should display apply fields button', async ({ page }) => {
    const applyBtn = page.getByTestId('apply-fields-btn')
    await expect(applyBtn).toBeVisible()
    await expect(applyBtn).toBeEnabled()
  })
})

test.describe('Schema Intent Step', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/sessions/a1b2c3d4-e5f6-7890-abcd-ef1234567890')
    await page.getByTestId('step-schema').click()
  })

  test('should display schema intent step content', async ({ page }) => {
    await expect(page.getByTestId('schema-intent-step')).toBeVisible()
  })

  test('should display question card when there is an ambiguity', async ({ page }) => {
    const questionCard = page.getByTestId('question-card')
    await expect(questionCard).toBeVisible()
  })

  test('should display question options', async ({ page }) => {
    const options = page.getByTestId('question-options')
    await expect(options).toBeVisible()
  })

  test('should display submit answer button', async ({ page }) => {
    const submitBtn = page.getByTestId('submit-answer-btn')
    await expect(submitBtn).toBeVisible()
  })

  test('should display schema table', async ({ page }) => {
    const schemaTable = page.getByTestId('schema-table')
    await expect(schemaTable).toBeVisible()
  })

  test('should display column rows in schema table', async ({ page }) => {
    await expect(page.getByTestId('column-row-order_id')).toBeVisible()
    await expect(page.getByTestId('column-row-amount')).toBeVisible()
  })

  test('should display add column button', async ({ page }) => {
    await expect(page.getByTestId('add-column-btn')).toBeVisible()
  })

  test('should display approve schema button', async ({ page }) => {
    const approveBtn = page.getByTestId('approve-schema-btn')
    await expect(approveBtn).toBeVisible()
  })
})

test.describe('Backtest Step', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/sessions/a1b2c3d4-e5f6-7890-abcd-ef1234567890')
    await page.getByTestId('step-backtest').click()
  })

  test('should display backtest step content', async ({ page }) => {
    await expect(page.getByTestId('backtest-step')).toBeVisible()
  })

  test('should display violation details when backtest is complete', async ({ page }) => {
    // The mock data shows backtest as complete with violations
    await expect(page.getByTestId('violation-0')).toBeVisible()
  })

  test('should display rerun button after backtest completes', async ({ page }) => {
    await expect(page.getByTestId('rerun-btn')).toBeVisible()
  })

  test('should display proceed button', async ({ page }) => {
    await expect(page.getByTestId('proceed-btn')).toBeVisible()
  })
})

test.describe('Publish/Run Step', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/sessions/a1b2c3d4-e5f6-7890-abcd-ef1234567890')
    await page.getByTestId('step-publish').click()
  })

  test('should display publish/run step content', async ({ page }) => {
    await expect(page.getByTestId('publish-run-step')).toBeVisible()
  })

  test('should display request run button when in run plan phase', async ({ page }) => {
    // The mock data shows phase as 'run_plan'
    await expect(page.getByTestId('request-run-btn')).toBeVisible()
  })
})
