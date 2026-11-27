<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import ModerationAccess from '#lib/components/moderation-access.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { moment, outcomeWord, reasonWord } from '#lib/moderation-words.ts';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();

	const href = (path: string) => resolve(localizeHref(path) as Path);
</script>

<svelte:head>
	<title>{m.history_title()} · {m.site_name()}</title>
</svelte:head>

<p class="mb-4 text-sm">
	<a href={href('/moderation')} class="underline-offset-4 hover:underline">{m.history_back()}</a>
</p>

{#if data.access !== 'granted' || !data.history}
	<h1 class="mb-6 text-2xl font-semibold">{m.history_title()}</h1>
	<ModerationAccess access={data.access} />
{:else}
	<p class="text-sm text-muted-foreground">{m.history_title()}</p>
	<h1 class="mb-6 text-2xl font-semibold">{data.history.title}</h1>
	<section class="mb-6 grid gap-2 rounded-lg border p-4" aria-labelledby="selection">
		<h2 id="selection" class="font-medium">{m.selection_title()}</h2>
		{#if form?.selection === 'selected'}
			<p role="status">{m.selection_added()}</p>
		{:else if form?.selection === 'unselected'}
			<p role="status">{m.selection_removed()}</p>
		{:else if form?.refused}
			<p role="alert" class="text-destructive">{m.selection_refused({ code: form.refused })}</p>
		{/if}
		{#if data.history.selection}
			<p class="text-sm">
				{m.selection_by({
					date: moment(data.history.selection.selected_at),
					moderator: data.history.selection.moderator
				})}
			</p>
			<form method="post" action="?/unselect">
				<Button type="submit" variant="outline">{m.selection_remove()}</Button>
			</form>
		{:else}
			<p class="text-sm">{m.selection_none()}</p>
			<form method="post" action="?/select">
				<Button type="submit">{m.selection_add()}</Button>
			</form>
		{/if}
	</section>
	{#if data.history.history.length === 0}
		<p>{m.history_empty()}</p>
	{/if}
	<ol class="grid gap-4 border-l pl-4">
		{#each data.history.history as entry, index (index)}
			<li class="grid gap-1 text-sm">
				<time datetime={entry.at} class="text-muted-foreground">{moment(entry.at)}</time>
				{#if entry.event === 'reported'}
					<p class="font-medium">{m.history_reported({ reason: reasonWord(entry.reason) })}</p>
					{#if entry.notice}<p>{m.moderation_notice(entry.notice)}</p>{/if}
					{#if entry.details}<p class="whitespace-pre-line">{entry.details}</p>{/if}
					{#if entry.findings}
						<p>{m.moderation_findings()}: {entry.findings}</p>
					{/if}
				{:else if entry.event === 'corrected'}
					<p class="font-medium">{m.history_corrected()}</p>
				{:else if entry.event === 'decided'}
					<p class="font-medium">
						{m.history_decided({ outcome: outcomeWord(entry.outcome), moderator: entry.moderator })}
					</p>
					<p class="whitespace-pre-line">{entry.reasons}</p>
				{:else if entry.event === 'appealed'}
					<p class="font-medium">
						{entry.appellant === 'author'
							? m.history_appealed_author()
							: m.history_appealed_reporter()}
					</p>
					<p class="whitespace-pre-line">{entry.statement}</p>
				{:else}
					<p class="font-medium">
						{m.history_appeal_decided({
							outcome: outcomeWord(entry.outcome),
							moderator: entry.moderator
						})}
					</p>
					<p class="whitespace-pre-line">{entry.reasons}</p>
				{/if}
			</li>
		{/each}
	</ol>
{/if}
