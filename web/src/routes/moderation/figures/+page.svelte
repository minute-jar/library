<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import ModerationAccess from '#lib/components/moderation-access.svelte';
	import { appellantWord, outcomeWord, reasonWord } from '#lib/moderation-words.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const href = (path: string) => resolve(localizeHref(path) as Path);
</script>

<svelte:head>
	<title>{m.figures_title({ year: data.year })} · {m.site_name()}</title>
</svelte:head>

<p class="mb-4 text-sm">
	<a href={href('/moderation')} class="underline-offset-4 hover:underline">{m.history_back()}</a>
</p>
<h1 class="mb-2 text-2xl font-semibold">{m.figures_title({ year: data.year })}</h1>

{#if data.access !== 'granted' || !data.figures}
	<ModerationAccess access={data.access} />
{:else}
	<p class="mb-4 text-muted-foreground">{m.figures_intro()}</p>
	<form method="get" class="mb-6 flex items-end gap-2">
		<label class="grid gap-1 text-sm">
			{m.figures_year()}
			<Input type="number" name="year" min={2000} max={9999} value={data.year} class="w-28" />
		</label>
		<Button type="submit" variant="outline">{m.figures_show()}</Button>
	</form>

	{@const figures = data.figures}
	<section class="mb-6">
		<h2 class="mb-2 text-lg font-semibold">{m.figures_reports()}</h2>
		{#if figures.reports.length === 0}
			<p>{m.figures_none()}</p>
		{:else}
			<table class="text-sm">
				<thead>
					<tr>
						<th class="pr-6 text-left">{m.figures_reason()}</th>
						<th class="text-right">{m.figures_count()}</th>
					</tr>
				</thead>
				<tbody>
					{#each figures.reports as row (row.reason)}
						<tr>
							<td class="pr-6">{reasonWord(row.reason)}</td>
							<td class="text-right tabular-nums">{row.received}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</section>

	<section class="mb-6">
		<h2 class="mb-2 text-lg font-semibold">{m.figures_decisions()}</h2>
		{#if figures.decisions.length === 0}
			<p>{m.figures_none()}</p>
		{:else}
			<table class="text-sm">
				<thead>
					<tr>
						<th class="pr-6 text-left">{m.figures_reason()}</th>
						<th class="pr-6 text-left">{m.figures_outcome()}</th>
						<th class="text-right">{m.figures_count()}</th>
					</tr>
				</thead>
				<tbody>
					{#each figures.decisions as row (`${row.reason}/${row.outcome}`)}
						<tr>
							<td class="pr-6">{reasonWord(row.reason)}</td>
							<td class="pr-6">{outcomeWord(row.outcome)}</td>
							<td class="text-right tabular-nums">{row.count}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</section>

	<section>
		<h2 class="mb-2 text-lg font-semibold">{m.figures_appeals()}</h2>
		{#if figures.appeals.length === 0}
			<p>{m.figures_none()}</p>
		{:else}
			<table class="text-sm">
				<thead>
					<tr>
						<th class="pr-6 text-left">{m.figures_appellant()}</th>
						<th class="pr-6 text-left">{m.figures_outcome()}</th>
						<th class="text-right">{m.figures_count()}</th>
					</tr>
				</thead>
				<tbody>
					{#each figures.appeals as row (`${row.appellant}/${row.outcome}`)}
						<tr>
							<td class="pr-6">{appellantWord(row.appellant)}</td>
							<td class="pr-6">{outcomeWord(row.outcome)}</td>
							<td class="text-right tabular-nums">{row.count}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</section>
{/if}
