// static/prompt_optimizer.js — General optimization rules
const PromptOptimizer = {
    optimize(text) {
        let result = text;

        // Remove filler words
        const fillers = /\b(basically|actually|just|really|very|quite|so|well|literally|essentially|simply|merely)\b/gi;
        result = result.replace(fillers, '');

        // Remove courtesy phrases
        const courtesy = /\b(please|thank you|thanks in advance|if possible|if you can|if you could)\b/gi;
        result = result.replace(courtesy, '');

        // Remove hedge words
        const hedges = /\b(maybe|perhaps|possibly|sort of|kind of|i guess|i think|i believe)\b/gi;
        result = result.replace(hedges, '');

        // Remove meta phrases
        const meta = /\b(i want you to|i need you to|i was wondering if|could you maybe|could you possibly|i'd like you to)\b/gi;
        result = result.replace(meta, '');

        // Verbose replacements
        const replacements = [
            [/in order to/gi, 'to'],
            [/due to the fact that/gi, 'because'],
            [/at this point in time/gi, 'now'],
            [/with regard to/gi, 'regarding'],
            [/has the ability to/gi, 'can'],
            [/it is necessary to/gi, 'must'],
            [/a large number of/gi, 'many'],
        ];
        for (const [pattern, replacement] of replacements) {
            result = result.replace(pattern, replacement);
        }

        // Normalize whitespace
        result = result.replace(/\s{2,}/g, ' ').trim();

        return result;
    },

    estimateTokens(text) {
        return Math.ceil(text.split(/\s+/).length * 1.3);
    }
};
