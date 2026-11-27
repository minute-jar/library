<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import ModerationAccess from '#lib/components/moderation-access.svelte';
	import {
		caseId,
		day,
		moment,
		outcomeWord,
		reasonWord,
		refusalWord
	} from '#lib/moderation-words.ts';
	import type { Case } from '#lib/server/moderation.ts';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();

	const href = (path: string) => resolve(localizeHref(path) as Path);
	const effects: Record<string, () => string> = {
		none: m.moderation_effect_none,
		switched_off: m.moderation_effect_switched_off,
		hidden: m.moderation_effect_hidden
	};
	const refusedCase = $derived(form && 'refused' in form ? form.case : null);
	const refused = $derived(form && 'refused' in form ? refusalWord(form.refused ?? '') : null);
	const typed = (id: string) => (form && 'refused' in form && form.case === id ? form.reasons : '');
	const REASONS = [
		'personal_data',
		'sexual_content',
		'illegal_content',
		'rights',
		'unsuitable_content',
		'wrong_answer',
		'other'
	];
	const actions: Record<Case['kind'], string> = {
		report: '?/decideReport',
		appeal: '?/decideAppeal',
		held_review: '?/decideShare',
		held_comment: '?/decideComment',
		comment_report: '?/decideCommentReport'
	};
	const fields: Record<Case['kind'], string> = {
		report: 'report',
		appeal: 'appeal',
		held_review: 'share',
		held_comment: 'comment',
		comment_report: 'comment_report'
	};
	const actionOf = (waiting: Case) => actions[waiting.kind];
	const fieldOf = (waiting: Case) => fields[waiting.kind];
	const outcomesOf = (waiting: Case) =>
		waiting.kind === 'report' || waiting.kind === 'comment_report'
			? ['upheld', 'rejected']
			: waiting.kind === 'appeal'
				? ['maintained', 'reversed']
				: ['published', 'refused'];
	/** Whether the moderator names the rule the review's findings were about. */
	const namesRule = (waiting: Case) =>
		waiting.kind === 'held_review' || waiting.kind === 'held_comment';
	const kindWord = (waiting: Case) =>
		waiting.kind === 'report'
			? m.moderation_report()
			: waiting.kind === 'held_review'
				? m.moderation_held_review()
				: waiting.kind === 'held_comment'
					? m.moderation_held_comment()
					: waiting.kind === 'comment_report'
						? m.moderation_comment_report()
						: waiting.appellant === 'author'
							? m.moderation_appeal_author()
							: m.moderation_appeal_reporter();
	const outcomeLabels: Record<string, () => string> = {
		upheld: m.outcome_upheld,
		rejected: m.outcome_rejected,
		maintained: m.outcome_maintained,
		reversed: m.outcome_reversed,
		published: m.outcome_publish_held,
		refused: m.outcome_refuse_held
	};
</script>

<svelte:head>
	<title>{m.moderation_title()} · {m.site_name()}</title>
</svelte:head>

<div class="mb-6 flex flex-wrap items-baseline justify-between gap-2">
	<h1 class="text-2xl font-semibold">{m.moderation_title()}</h1>
	{#if data.access === 'granted'}
		<a href={href('/moderation/figures')} class="text-sm underline-offset-4 hover:underline">
			{m.figures_link()}
		</a>
	{/if}
</div>

{#if data.access !== 'granted'}
	<ModerationAccess access={data.access} />
{:else}
	<p class="mb-6 text-muted-foreground">{m.moderation_intro()}</p>
	{#if form && 'decided' in form}
		<p role="status" class="mb-4">{m.moderation_decided()}</p>
	{/if}
	{#if data.cases.length === 0}
		<p>{m.moderation_empty()}</p>
	{/if}
	<div class="grid gap-6">
		{#each data.cases as waiting (caseId(waiting))}
			{@const id = caseId(waiting)}
			<article aria-labelledby="case-{id}">
				<Card.Root>
					<Card.Header>
						<Card.Description class="flex flex-wrap gap-2">
							<Badge variant="secondary">{kindWord(waiting)}</Badge>
							{#if waiting.kind !== 'held_review' && waiting.kind !== 'held_comment'}
								<Badge variant="outline">{reasonWord(waiting.reason)}</Badge>
							{/if}
						</Card.Description>
						<Card.Title>
							<h2 id="case-{id}" class="text-lg">
								{waiting.title ?? m.moderation_untitled()}
							</h2>
						</Card.Title>
					</Card.Header>
					<Card.Content class="grid gap-3 text-sm">
						<p class="text-muted-foreground">
							{m.moderation_received({ date: moment(waiting.received_at) })} ·
							<strong>{m.moderation_due({ date: moment(waiting.due_at) })}</strong>
						</p>
						{#if waiting.kind === 'report'}
							<p>{m.moderation_version({ version: waiting.version })}</p>
							<p>{effects[waiting.effect]()}</p>
							{#if waiting.notice}
								<p>{m.moderation_notice(waiting.notice)}</p>
							{/if}
							{#if waiting.details}
								<div>
									<h3 class="font-medium">{m.moderation_details()}</h3>
									<p class="whitespace-pre-line">{waiting.details}</p>
								</div>
							{/if}
							{#if waiting.findings}
								<div>
									<h3 class="font-medium">{m.moderation_findings()}</h3>
									<p class="whitespace-pre-line">{waiting.findings}</p>
								</div>
							{/if}
						{:else if waiting.kind === 'held_review'}
							<p>{m.moderation_held_by({ pseudonym: waiting.pseudonym })}</p>
							<div>
								<h3 class="font-medium">{m.moderation_findings()}</h3>
								<p class="whitespace-pre-line">{waiting.findings}</p>
							</div>
							<details>
								<summary class="cursor-pointer font-medium">{m.moderation_held_content()}</summary>
								<pre class="mt-2 overflow-x-auto rounded-md bg-muted p-3 text-xs">{JSON.stringify(
										waiting.content,
										null,
										2
									)}</pre>
							</details>
						{:else if waiting.kind === 'held_comment' || waiting.kind === 'comment_report'}
							<p>{m.moderation_comment_by({ pseudonym: waiting.pseudonym })}</p>
							<blockquote class="border-l-2 pl-3 whitespace-pre-line">{waiting.text}</blockquote>
							{#if waiting.kind === 'held_comment'}
								<div>
									<h3 class="font-medium">{m.moderation_findings()}</h3>
									<p class="whitespace-pre-line">{waiting.findings}</p>
								</div>
							{:else}
								{#if waiting.hidden}<p>{m.moderation_comment_hidden()}</p>{/if}
								{#if waiting.details}
									<div>
										<h3 class="font-medium">{m.moderation_details()}</h3>
										<p class="whitespace-pre-line">{waiting.details}</p>
									</div>
								{/if}
							{/if}
						{:else}
							<div>
								<p>
									{m.moderation_appealed({
										moderator: waiting.decision.moderator,
										date: day(waiting.decision.decided_at)
									})}
									<strong>{outcomeWord(waiting.decision.outcome)}</strong>
								</p>
								<p class="whitespace-pre-line">{waiting.decision.reasons}</p>
							</div>
							<div>
								<h3 class="font-medium">{m.moderation_statement()}</h3>
								<p class="whitespace-pre-line">{waiting.statement}</p>
							</div>
						{/if}
						{#if waiting.kind === 'report' || waiting.kind === 'appeal'}
							<p>
								<a
									href={href(`/moderation/items/${waiting.item}`)}
									class="underline underline-offset-4"
								>
									{m.moderation_history_link()}
								</a>
							</p>
						{/if}

						<form method="post" action={actionOf(waiting)} class="grid gap-3 border-t pt-3">
							<input type="hidden" name={fieldOf(waiting)} value={id} />
							{#if refusedCase === id && refused}
								<p role="alert" class="text-destructive">{refused}</p>
							{/if}
							<fieldset class="grid gap-2">
								<legend class="mb-1 font-medium">{m.moderation_outcome()}</legend>
								{#each outcomesOf(waiting) as outcome (outcome)}
									<label class="flex gap-2">
										<input type="radio" name="outcome" value={outcome} required />
										{outcomeLabels[outcome]()}
									</label>
								{/each}
							</fieldset>
							{#if namesRule(waiting)}
								<label class="grid gap-1">
									{m.moderation_held_reason()}
									<select name="reason" required class="rounded-md border bg-transparent px-3 py-2">
										{#each REASONS as reason (reason)}
											<option value={reason}>{reasonWord(reason)}</option>
										{/each}
									</select>
								</label>
							{/if}
							<label class="grid gap-1">
								{m.moderation_reasons()}
								<textarea
									name="reasons"
									required
									rows="3"
									class="rounded-md border bg-transparent px-3 py-2"
									value={typed(id)}></textarea>
							</label>
							<div>
								<Button type="submit">{m.moderation_decide()}</Button>
							</div>
						</form>
					</Card.Content>
				</Card.Root>
			</article>
		{/each}
	</div>
{/if}
