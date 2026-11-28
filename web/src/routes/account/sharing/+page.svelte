<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { getLocale, localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();

	const href = (path: string) => resolve(localizeHref(path) as Path);

	const refusals: Record<string, () => string> = {
		acceptance_required: m.refused_acceptance_required,
		outdated_sharing_terms: m.refused_outdated_sharing_terms,
		statement_required: m.refused_statement_required_appeal,
		already_appealed: m.refused_already_appealed,
		too_late: m.refused_too_late
	};
	const appealedReport = $derived(form && 'appealed' in form ? form.appealed : null);
	const refusedReport = $derived(form && 'report' in form ? form.report : null);
	const code = $derived(form && 'refused' in form ? form.refused : null);
	const refused = $derived(code ? (refusals[code] ?? m.refused_other)() : null);
	const terms = $derived(data.sharing_terms);
	const accepted = $derived(terms.accepted === terms.current);
	const quota = $derived(data.quota);
	const day = (date: string) =>
		new Intl.DateTimeFormat(getLocale(), { dateStyle: 'long', timeZone: 'UTC' }).format(
			new Date(date)
		);
	const moment = (date: string) =>
		new Intl.DateTimeFormat(getLocale(), {
			dateStyle: 'long',
			timeStyle: 'short',
			timeZone: 'UTC',
			timeZoneName: 'short'
		}).format(new Date(date));
</script>

<svelte:head>
	<title>{m.sharing_title()} · {m.site_name()}</title>
</svelte:head>

<h1 class="mb-1 text-2xl font-semibold">{m.sharing_title()}</h1>
<p class="mb-6 max-w-xl text-muted-foreground">{m.sharing_why()}</p>

{#if refused && !refusedReport}
	<p role="alert" class="mb-4 text-destructive">{refused}</p>
{/if}

<Card.Root class="mb-6 max-w-xl">
	<Card.Header>
		<Card.Title><h2>{m.terms_title()}</h2></Card.Title>
	</Card.Header>
	<Card.Content class="grid gap-4">
		<p>{m.terms_intro()}</p>
		<ul class="grid list-disc gap-2 pl-5">
			<li>{m.terms_rights()}</li>
			<li>{m.terms_compliant()}</li>
			<li>{m.terms_answer()}</li>
			<li>{m.terms_licence()}</li>
			<li>{m.terms_review()}</li>
		</ul>
		{#if accepted}
			<p role="status">{m.terms_accepted()}</p>
		{:else}
			{#if terms.accepted}
				<p>{m.terms_changed()}</p>
			{/if}
			<form method="post" action="?/accept" class="grid gap-4">
				<input type="hidden" name="version" value={terms.current} />
				<label class="flex items-center gap-2">
					<input type="checkbox" name="accepted" required />
					{m.terms_accept_label()}
				</label>
				<div>
					<Button type="submit">{m.terms_submit()}</Button>
				</div>
			</form>
		{/if}
	</Card.Content>
</Card.Root>

<section class="max-w-xl">
	<h2 class="mb-2 text-xl font-semibold">{m.shares_title()}</h2>
	<p class="mb-1">
		{quota.next_share_at
			? m.quota_reached({ per_day: quota.shares_per_day, time: moment(quota.next_share_at) })
			: m.quota_left({
					left: quota.shares_per_day - quota.shared_today,
					per_day: quota.shares_per_day
				})}
	</p>
	<p class="mb-4 text-sm text-muted-foreground">{m.quota_why()}</p>

	{#if data.shares.length === 0}
		<p>{m.shares_none()}</p>
	{:else}
		<ul class="grid gap-4">
			{#each data.shares as share (share.share)}
				<li class="rounded-lg border p-4">
					<h3 class="font-medium">
						{#if share.outcome === 'published' && share.item}
							<a href={href(`/items/${share.item}`)} class="underline">{share.title}</a>
						{:else}
							{share.title}
						{/if}
					</h3>
					<p class="text-sm text-muted-foreground">{m.shared_on({ date: day(share.shared_at) })}</p>
					{#if share.outcome === 'published'}
						<p>{m.outcome_published({ version: share.version ?? 1 })}</p>
					{:else if share.outcome === 'held'}
						<p>{m.outcome_held()}</p>
					{:else}
						<p>{m.outcome_refused()}</p>
						<ul class="mt-2 grid list-disc gap-1 pl-5">
							{#each share.reasons as reason, n (n)}
								<li>
									<span class="font-medium">
										{reason.question === null
											? m.reason_whole_item()
											: m.reason_question({ question: reason.question })}:
									</span>
									{reason.explanation}
								</li>
							{/each}
						</ul>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<section class="mt-8 max-w-xl">
	<h2 class="mb-2 text-xl font-semibold">{m.told_title()}</h2>
	{#if data.notifications.length === 0}
		<p>{m.told_none()}</p>
	{:else}
		<ul class="grid gap-4">
			{#each data.notifications as told (told.report + told.at)}
				<li class="grid gap-2 rounded-lg border p-4">
					<h3 class="font-medium">{told.title}</h3>
					<p class="text-sm text-muted-foreground">{day(told.at)}</p>
					{#if told.details}
						<p class="whitespace-pre-line">{told.details}</p>
					{/if}
					{#if told.appeal}
						<p>
							{told.appeal.outcome === 'reversed'
								? m.told_appeal_reversed()
								: m.told_appeal_maintained()}
						</p>
						<p class="whitespace-pre-line">{told.appeal.reasons}</p>
					{:else if told.decision}
						<p>
							{told.decision.outcome === 'upheld' ? m.told_upheld() : m.told_rejected()}
						</p>
						<p class="whitespace-pre-line">{told.decision.reasons}</p>
						{#if appealedReport === told.report}
							<p role="status">{m.told_appealed()}</p>
						{:else}
							<form method="post" action="?/appeal" class="grid gap-2">
								<input type="hidden" name="report" value={told.report} />
								<label class="grid gap-1">
									{m.told_appeal_label()}
									<textarea
										name="statement"
										required
										rows="3"
										class="rounded-md border bg-transparent px-3 py-2"></textarea>
								</label>
								<div>
									<Button type="submit" variant="outline">{m.told_appeal_submit()}</Button>
								</div>
							</form>
						{/if}
					{:else}
						<p>{m.told_to_correct()}</p>
					{/if}
					{#if refusedReport === told.report && refused}
						<p role="alert" class="text-destructive">{refused}</p>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<p class="mt-8">
	<a href={href('/account')} class="text-sm underline">{m.account_title()}</a>
</p>
