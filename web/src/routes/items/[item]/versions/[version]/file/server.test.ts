import { describe, expect, it } from 'vitest';
import { fakeApi, verbs, verbsFile } from '#lib/server/testing/fake-api.ts';
import { GET } from './+server.ts';

type Event = Parameters<typeof GET>[0];

function download(item: string, version: string, api = fakeApi()) {
	const event = { params: { item, version }, fetch: api.fetch };
	return GET(event as unknown as Event);
}

describe('the download of an item file', () => {
	it('hands over the file the API made, byte for byte, as an attachment', async () => {
		const answer = await download(verbs.item, '2');

		expect(answer.status).toBe(200);
		expect(answer.headers.get('content-type')).toBe('application/json');
		expect(answer.headers.get('content-disposition')).toBe(
			`attachment; filename="minute-jar-${verbs.item}-2.json"`
		);
		expect(await answer.text()).toBe(verbsFile);
	});

	it('answers 404 for a version the catalogue does not show', async () => {
		await expect(download(verbs.item, '3')).rejects.toMatchObject({ status: 404 });
	});
});
