import { expect, test } from '@playwright/test';

test('a visitor browses and searches the catalogue, in English and in French', async ({ page }) => {
	await page.goto('/');
	await expect(page.getByRole('heading', { name: 'Irregular verbs, part 1' })).toBeVisible();
	await expect(page.getByText('2 shared items')).toBeVisible();

	await page.getByRole('searchbox').fill('verbs');
	await page.getByRole('button', { name: 'Search' }).click();
	await expect(page).toHaveURL(/[?&]q=verbs/);
	await expect(page.getByRole('searchbox')).toHaveValue('verbs');

	await page.getByRole('link', { name: 'Français' }).click();
	await expect(page).toHaveURL(/\/fr\/?\?.*q=verbs/);
	await expect(page.locator('html')).toHaveAttribute('lang', 'fr');
	await expect(page.getByText('2 éléments partagés')).toBeVisible();
	await expect(page.getByText(/Parcours\s+·\s+par Minute Jar/)).toBeVisible();
});
