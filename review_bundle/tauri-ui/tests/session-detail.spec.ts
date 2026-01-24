import { test, expect } from '@playwright/test'

test.describe('Session Detail Screen', () => {
  test.beforeEach(async ({ page }) => {
    // Navigate to an existing session
    await page.goto('/sessions/a1b2c3d4-e5f6-7890-abcd-ef1234567890')
  })

  test('should display session detail screen', async ({ page }) => {
    await expect(page.getByTestId('session-detail-screen')).toBeVisible()
  })

  test('should display back button', async ({ page }) => {
    const backBtn = page.getByTestId('back-btn')
    await expect(backBtn).toBeVisible()
  })

  test('should navigate back to sessions list when clicking back', async ({ page }) => {
    await page.getByTestId('back-btn').click()
    await expect(page).toHaveURL('/sessions')
  })

  test('should display session intent as title', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Process all sales CSV files from Q4' })).toBeVisible()
  })

  test('should display workflow stepper', async ({ page }) => {
    const stepper = page.getByTestId('workflow-stepper')
    await expect(stepper).toBeVisible()
  })

  test('should display all workflow steps', async ({ page }) => {
    await expect(page.getByTestId('step-selection')).toBeVisible()
    await expect(page.getByTestId('step-tags')).toBeVisible()
    await expect(page.getByTestId('step-pathfields')).toBeVisible()
    await expect(page.getByTestId('step-schema')).toBeVisible()
    await expect(page.getByTestId('step-backtest')).toBeVisible()
    await expect(page.getByTestId('step-publish')).toBeVisible()
  })

  test('should display session info bar', async ({ page }) => {
    const infoBar = page.getByTestId('session-info')
    await expect(infoBar).toBeVisible()
    await expect(infoBar).toContainText('files selected')
  })

  test('should display step content area', async ({ page }) => {
    const stepContent = page.getByTestId('step-content')
    await expect(stepContent).toBeVisible()
  })

  test('should display save button', async ({ page }) => {
    const saveBtn = page.getByTestId('save-btn')
    await expect(saveBtn).toBeVisible()
  })

  test('should allow clicking on completed workflow steps', async ({ page }) => {
    // The selection step should be clickable (it's before the current step)
    const selectionStep = page.getByTestId('step-selection')
    await expect(selectionStep).toBeEnabled()
  })
})

test.describe('New Session Screen', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/sessions/new')
  })

  test('should display new session header', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'New Session' })).toBeVisible()
    await expect(page.getByText('Create a new intent pipeline')).toBeVisible()
  })

  test('should not display session info bar for new session', async ({ page }) => {
    // Session info bar should not be visible for new sessions
    const infoBar = page.getByTestId('session-info')
    await expect(infoBar).not.toBeVisible()
  })
})
