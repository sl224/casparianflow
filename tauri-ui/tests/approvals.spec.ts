import { test, expect } from '@playwright/test'

test.describe('Approvals Screen', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/approvals')
  })

  test('should display page header with title and subtitle', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Approvals' })).toBeVisible()
    await expect(page.getByText('Review and approve pending operations')).toBeVisible()
  })

  test('should display approval stats', async ({ page }) => {
    const stats = page.getByTestId('approval-stats')
    await expect(stats).toBeVisible()

    // Check all stats
    await expect(page.getByTestId('stat-pending')).toBeVisible()
    await expect(page.getByTestId('stat-approved')).toBeVisible()
    await expect(page.getByTestId('stat-rejected')).toBeVisible()
    await expect(page.getByTestId('stat-expired')).toBeVisible()
  })

  test('should display correct stat values', async ({ page }) => {
    const pending = page.getByTestId('stat-pending')
    await expect(pending.getByText('5')).toBeVisible()

    const approved = page.getByTestId('stat-approved')
    await expect(approved.getByText('23')).toBeVisible()

    const rejected = page.getByTestId('stat-rejected')
    await expect(rejected.getByText('2')).toBeVisible()

    const expired = page.getByTestId('stat-expired')
    await expect(expired.getByText('8')).toBeVisible()
  })

  test('should display pending approvals list', async ({ page }) => {
    const approvalsList = page.getByTestId('approvals-list')
    await expect(approvalsList).toBeVisible()

    // Check approval rows are present
    await expect(page.getByTestId('approval-row-0')).toBeVisible()
    await expect(page.getByTestId('approval-row-1')).toBeVisible()
    await expect(page.getByTestId('approval-row-2')).toBeVisible()
    await expect(page.getByTestId('approval-row-3')).toBeVisible()
  })

  test('should display approval details in each row', async ({ page }) => {
    const firstRow = page.getByTestId('approval-row-0')

    await expect(firstRow.getByText('Run parser on /data/sales')).toBeVisible()
    await expect(firstRow.getByText('fix_parser')).toBeVisible()
    await expect(firstRow.getByText('247')).toBeVisible()
    await expect(firstRow.getByText('in 45 min')).toBeVisible()
  })

  test('should have approve and reject buttons for each approval', async ({ page }) => {
    // Check first approval row has both buttons
    await expect(page.getByTestId('approve-btn-0')).toBeVisible()
    await expect(page.getByTestId('reject-btn-0')).toBeVisible()

    // Check second approval row
    await expect(page.getByTestId('approve-btn-1')).toBeVisible()
    await expect(page.getByTestId('reject-btn-1')).toBeVisible()
  })

  test('approve button should be clickable', async ({ page }) => {
    const approveBtn = page.getByTestId('approve-btn-0')
    await expect(approveBtn).toBeEnabled()
    await expect(approveBtn).toHaveText('Approve')
  })

  test('reject button should be clickable', async ({ page }) => {
    const rejectBtn = page.getByTestId('reject-btn-0')
    await expect(rejectBtn).toBeEnabled()
    await expect(rejectBtn).toHaveText('Reject')
  })

  test('should highlight urgent approvals', async ({ page }) => {
    // The first approval expires "in 45 min" which is marked as urgent
    const firstRow = page.getByTestId('approval-row-0')
    const expiresText = firstRow.getByText('in 45 min')

    // Should have warning color class
    await expect(expiresText).toHaveClass(/text-warning/)
  })
})
