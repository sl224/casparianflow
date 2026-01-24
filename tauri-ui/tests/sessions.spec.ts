import { test, expect } from '@playwright/test'

test.describe('Sessions Screen', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/sessions')
  })

  test('should display sessions screen with correct header', async ({ page }) => {
    await expect(page.getByTestId('sessions-screen')).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Intent Sessions' })).toBeVisible()
    await expect(page.getByText('Manage data pipeline workflows')).toBeVisible()
  })

  test('should display header action buttons', async ({ page }) => {
    await expect(page.getByTestId('refresh-btn')).toBeVisible()
    await expect(page.getByTestId('new-session-btn')).toBeVisible()
    await expect(page.getByTestId('new-session-btn')).toBeEnabled()
  })

  test('should display stats row with all stats', async ({ page }) => {
    const statsRow = page.getByTestId('session-stats')
    await expect(statsRow).toBeVisible()

    await expect(page.getByTestId('stat-active')).toBeVisible()
    await expect(page.getByTestId('stat-awaiting')).toBeVisible()
    await expect(page.getByTestId('stat-complete')).toBeVisible()
    await expect(page.getByTestId('stat-failed')).toBeVisible()
  })

  test('should display sessions list', async ({ page }) => {
    const sessionsList = page.getByTestId('sessions-list')
    await expect(sessionsList).toBeVisible()

    // Should have at least one session row
    await expect(page.getByTestId('session-row-0')).toBeVisible()
  })

  test('should display session details in rows', async ({ page }) => {
    const firstRow = page.getByTestId('session-row-0')
    await expect(firstRow).toBeVisible()

    // Check row has intent text
    await expect(firstRow).toContainText('Process all sales CSV files from Q4')
  })

  test('should show question badge for sessions requiring human input', async ({ page }) => {
    // Session 0 should have a question badge (hasQuestion: true in mock data)
    const questionBadge = page.getByTestId('question-badge-0')
    await expect(questionBadge).toBeVisible()
  })

  test('should display state filter dropdown', async ({ page }) => {
    const stateFilter = page.getByTestId('state-filter')
    await expect(stateFilter).toBeVisible()
  })

  test('should have clickable session rows', async ({ page }) => {
    const firstRow = page.getByTestId('session-row-0')
    await expect(firstRow).toHaveClass(/table-row-clickable/)
  })

  test('should have open button for each session', async ({ page }) => {
    const openBtn = page.getByTestId('open-btn-0')
    await expect(openBtn).toBeVisible()
    await expect(openBtn).toBeEnabled()
  })

  test('should navigate to new session when clicking New Session button', async ({ page }) => {
    await page.getByTestId('new-session-btn').click()
    await expect(page).toHaveURL(/\/sessions\/new/)
  })

  test('should navigate to session detail when clicking a session row', async ({ page }) => {
    await page.getByTestId('session-row-0').click()
    await expect(page).toHaveURL(/\/sessions\//)
  })

  test('should navigate to session detail when clicking open button', async ({ page }) => {
    await page.getByTestId('open-btn-0').click()
    await expect(page).toHaveURL(/\/sessions\//)
  })
})
