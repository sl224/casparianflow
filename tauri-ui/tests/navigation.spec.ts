import { test, expect } from '@playwright/test'

test.describe('Navigation', () => {
  test('should display sidebar with all navigation items', async ({ page }) => {
    await page.goto('/')

    const sidebar = page.getByTestId('sidebar')
    await expect(sidebar).toBeVisible()

    // Check all nav items are present
    await expect(page.getByTestId('nav-home')).toBeVisible()
    await expect(page.getByTestId('nav-discover')).toBeVisible()
    await expect(page.getByTestId('nav-parsers')).toBeVisible()
    await expect(page.getByTestId('nav-jobs')).toBeVisible()
    await expect(page.getByTestId('nav-approvals')).toBeVisible()
    await expect(page.getByTestId('nav-query')).toBeVisible()
    await expect(page.getByTestId('nav-settings')).toBeVisible()
  })

  test('should navigate to Home page by default', async ({ page }) => {
    await page.goto('/')

    await expect(page.getByTestId('home-screen')).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Home' })).toBeVisible()
  })

  test('should navigate to Approvals page when clicking nav item', async ({ page }) => {
    await page.goto('/')

    await page.getByTestId('nav-approvals').click()

    await expect(page.getByTestId('approvals-screen')).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Approvals' })).toBeVisible()
  })

  test('should navigate to Query page when clicking nav item', async ({ page }) => {
    await page.goto('/')

    await page.getByTestId('nav-query').click()

    await expect(page.getByTestId('query-screen')).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Query Console' })).toBeVisible()
  })

  test('should highlight active navigation item', async ({ page }) => {
    await page.goto('/home')

    const homeNav = page.getByTestId('nav-home')
    await expect(homeNav).toHaveClass(/active/)

    await page.getByTestId('nav-approvals').click()

    const approvalsNav = page.getByTestId('nav-approvals')
    await expect(approvalsNav).toHaveClass(/active/)
    await expect(homeNav).not.toHaveClass(/active/)
  })
})
