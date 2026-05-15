
struct TokenEfficiencyScorer;

impl TokenEfficiencyScorer {
    fn calculate_raw(
        input_tokens: u32,
        output_tokens: u32,
    ) -> f32 {
        if input_tokens == 0 {
            return 0.0;
        }

        let compression_ratio = output_tokens as f32 / input_tokens as f32;
        let savings = 1.0 - compression_ratio;

        let base = if savings < 0.0 {
            (0.3 + savings * 0.4).max(0.1)
        } else if savings <= 0.1 {
            0.3 + savings * 2.0 // 0.3 to 0.5
        } else if savings <= 0.5 {
            0.5 + (savings - 0.1) * 0.75 // 0.5 to 0.8
        } else if savings <= 0.6 {
            0.8 + (savings - 0.5) * 1.0 // 0.8 to 0.9
        } else if savings <= 0.8 {
            0.9 - (savings - 0.6) * 0.5 // 0.9 to 0.8
        } else {
            0.8 - (savings - 0.8) * 2.0 // 0.8 down to 0.4
        };

        base
    }

    pub fn score(input: u32, output: u32) -> f32 {
        let raw = Self::calculate_raw(input, output);
        (raw * 10.0).min(10.0).max(0.0)
    }
}

fn main() {
    let cases = vec![
        (100, 150, "Expansion (-50% savings)"),
        (100, 100, "No compression (0% savings)"),
        (100, 90, "Light compression (10% savings)"),
        (100, 80, "Light compression (20% savings)"),
        (100, 50, "Good compression (50% savings)"),
        (100, 40, "Sweet spot peak (60% savings)"),
        (100, 20, "Heavy compression (80% savings)"),
        (100, 10, "Over-compression (90% savings)"),
    ];

    for (inp, out, desc) in cases {
        let score = TokenEfficiencyScorer::score(inp, out);
        println!("{}: Input={}, Output={}, Score={:.2}", desc, inp, out, score);
    }
}
