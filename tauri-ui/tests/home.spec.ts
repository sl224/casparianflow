import { test, expect } from '@playwright/test'

test.describe('Home Screen', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/home')
  })

  test('should display page header with title and subtitle', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Home' })).toBeVisible()
    await expect(page.getByText('Readiness Board - Output-first triage dashboard')).toBeVisible()
  })

  test('should display stats row with all stat cards', async ({ page }) => {
    const statsRow = page.getByTestId('stats-row')
    await expect(statsRow).toBeVisible()

    // Check all stats are present
    await expect(page.getByTestId('stat-ready-outputs')).toBeVisible()
    await expect(page.getByTestId('stat-running-jobs')).toBeVisible()
    await expect(page.getByTestId('stat-quarantined')).toBeVisible()
    await expect(page.getByTestId('stat-failed-jobs')).toBeVisible()
  })

  test('should display correct stat values', async ({ page }) => {
    // Ready Outputs
    const readyOutputs = page.getByTestId('stat-ready-outputs')
    await expect(readyOutputs.getByText('12')).toBeVisible()
    await expect(readyOutputs.getByText('Tables ready to query')).toBeVisible()

    // Running Jobs
    const runningJobs = page.getByTestId('stat-running-jobs')
    await expect(runningJobs.getByText('3')).toBeVisible()

    // Failed Jobs
    const failedJobs = page.getByTestId('stat-failed-jobs')
    await expect(failedJobs.getByText('2')).toBeVisible()
  })

  test('should display ready outputs list', async ({ page }) => {
    const outputsList = page.getByTestId('ready-outputs-list')
    await expect(outputsList).toBeVisible()

    // Check outputs are listed
    await expect(page.getByTestId('output-fix_order_lifecycle')).toBeVisible()
    await expect(page.getByTestId('output-fix_executions')).toBeVisible()
    await expect(page.getByTestId('output-hl7_observations')).toBeVisible()
  })

  test('should display active runs with progress bars', async ({ page }) => {
    const activeRuns = page.getByTestId('active-runs-list')
    await expect(activeRuns).toBeVisible()

    await expect(page.getByText('Fidesrex_bc_parser')).toBeVisible()
    await expect(page.getByText('67%')).toBeVisible()
    await expect(page.getByText('MT_multi_type')).toBeVisible()
    await expect(page.getByText('23%')).toBeVisible()
  })

  test('should display quick action buttons', async ({ page }) => {
    await expect(page.getByTestId('btn-open-files')).toBeVisible()
    await expect(page.getByTestId('btn-scan-folder')).toBeVisible()
    await expect(page.getByTestId('btn-query-output')).toBeVisible()
  })

  test('quick action buttons should be clickable', async ({ page }) => {
    const openFilesBtn = page.getByTestId('btn-open-files')
    await expect(openFilesBtn).toBeEnabled()

    const scanFolderBtn = page.getByTestId('btn-scan-folder')
    await expect(scanFolderBtn).toBeEnabled()

    const queryOutputBtn = page.getByTestId('btn-query-output')
    await expect(queryOutputBtn).toBeEnabled()
  })
})
