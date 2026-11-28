import { expect, test } from '@playwright/test';

test('a moderator signs in with a passkey and decides a report with reasons', async ({ page }) => {
	await page.goto('/moderation');
	await page.getByRole('link', { name: 'Sign in with a passkey' }).click();

	await expect(page).toHaveURL(/\/moderation$/);
	await expect(page.getByRole('heading', { name: 'Moderation queue' })).toBeVisible();
	const report = page.getByRole('article', { name: 'Irregular verbs, part 1' });
	await expect(report.getByText('Formal notice from Example Publishing')).toBeVisible();
	await expect(page.getByRole('article', { name: 'Les fractions, CM1' })).toBeVisible();

	await report.getByRole('radio', { name: 'Uphold: withdraw the item' }).check();
	await report
		.getByRole('textbox', { name: 'Reasons, sent to the author and the reporter' })
		.fill('The questions copy the workbook.');
	await report.getByRole('button', { name: 'Decide' }).click();

	await expect(page.getByText('Decided. The author and the reporter are told.')).toBeVisible();
	await expect(page.getByRole('article', { name: 'Irregular verbs, part 1' })).toHaveCount(0);

	await page.goto('/moderation/items/0123456789abcdef0123456789abcdef');
	await expect(page.getByRole('heading', { name: 'Irregular verbs, part 1' })).toBeVisible();
	await expect(page.getByText('Upheld by Moderator')).toBeVisible();
	await expect(page.getByText('The questions copy the workbook.')).toBeVisible();
});

test('a moderator adds an item to the Selection and removes it', async ({ page }) => {
	await page.goto('/moderation/items/0123456789abcdef0123456789abcdef');
	await page.getByRole('link', { name: 'Sign in with a passkey' }).click();
	await page.goto('/moderation/items/0123456789abcdef0123456789abcdef');

	await expect(page.getByText('This shared item is not in the Selection.')).toBeVisible();
	await page.getByRole('button', { name: 'Add to the Selection' }).click();

	await expect(page.getByText('Added to the Selection. The index is signed again.')).toBeVisible();
	await expect(page.getByText(/In the Selection since .*, chosen by Moderator\./)).toBeVisible();
	await page.getByRole('button', { name: 'Remove from the Selection' }).click();

	await expect(page.getByText('Removed from the Selection.', { exact: false })).toBeVisible();
	await expect(page.getByText('This shared item is not in the Selection.')).toBeVisible();
});
