use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::ResourceBudget;
use crate::types::{PdfDictionary, PdfStream, PdfValue};

/// Experimental PDF Function parser for the supported function types.
#[derive(Debug, Clone)]
pub enum PdfFunction {
    Type0(SampledFunction),     // Sampled function
    Type2(ExponentialFunction), // Exponential interpolation
    Type3(StitchingFunction),   // Stitching function
    Type4(PostScriptFunction),  // PostScript calculator
}

#[derive(Debug, Clone)]
pub struct SampledFunction {
    pub domain: Vec<(f64, f64)>,
    pub range: Vec<(f64, f64)>,
    pub size: Vec<u32>,
    pub bits_per_sample: u32,
    pub order: u32,
    pub encode: Vec<(f64, f64)>,
    pub decode: Vec<(f64, f64)>,
    pub samples: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct ExponentialFunction {
    pub domain: Vec<(f64, f64)>,
    pub range: Vec<(f64, f64)>,
    pub c0: Vec<f64>,
    pub c1: Vec<f64>,
    pub n: f64,
}

#[derive(Debug, Clone)]
pub struct StitchingFunction {
    pub domain: Vec<(f64, f64)>,
    pub range: Vec<(f64, f64)>,
    pub functions: Vec<Box<PdfFunction>>,
    pub bounds: Vec<f64>,
    pub encode: Vec<(f64, f64)>,
}

#[derive(Debug, Clone)]
pub struct PostScriptFunction {
    pub domain: Vec<(f64, f64)>,
    pub range: Vec<(f64, f64)>,
    pub code: String,
}

pub struct FunctionParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    budget: ResourceBudget,
}

impl<'a> FunctionParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        Self::new_with_budget(ast, resolver, &ResourceBudget::default())
    }

    pub fn new_with_budget(
        ast: &'a mut PdfAstGraph,
        resolver: &'a ObjectNodeMap,
        budget: &ResourceBudget,
    ) -> Self {
        FunctionParser {
            ast,
            resolver,
            budget: budget.clone(),
        }
    }

    pub fn parse_function(&mut self, func_value: &PdfValue) -> Option<(NodeId, PdfFunction)> {
        match func_value {
            PdfValue::Dictionary(dict) => self.parse_function_dict(dict, None),
            PdfValue::Stream(stream) => self.parse_function_dict(&stream.dict, Some(stream)),
            PdfValue::Reference(obj_id) => {
                if let Some(node_id) = self.resolver.get_node_id(&obj_id.id()) {
                    if let Some(node) = self.ast.get_node(node_id) {
                        match &node.value {
                            PdfValue::Dictionary(dict) => {
                                let dict = dict.clone();
                                return self.parse_function_dict(&dict, None);
                            }
                            PdfValue::Stream(stream) => {
                                let dict = stream.dict.clone();
                                let stream = stream.clone();
                                return self.parse_function_dict(&dict, Some(&stream));
                            }
                            _ => {}
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn parse_function_dict(
        &mut self,
        dict: &PdfDictionary,
        stream: Option<&PdfStream>,
    ) -> Option<(NodeId, PdfFunction)> {
        let func_type = dict.get("FunctionType").and_then(|v| v.as_integer())?;

        let function = match func_type {
            0 => self.parse_type0_function(dict, stream?)?,
            2 => self.parse_type2_function(dict)?,
            3 => self.parse_type3_function(dict)?,
            4 => self.parse_type4_function(dict, stream?)?,
            _ => return None,
        };

        // Create AST node
        self.budget.consume_node().ok()?;
        let mut node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::Function,
            if let Some(s) = stream {
                PdfValue::Stream(s.clone())
            } else {
                PdfValue::Dictionary(dict.clone())
            },
        );

        // Add metadata
        node.metadata
            .set_property("function_type".to_string(), func_type.to_string());

        match &function {
            PdfFunction::Type0(f) => {
                node.metadata
                    .set_property("subtype".to_string(), "Sampled".to_string());
                node.metadata
                    .set_property("bits_per_sample".to_string(), f.bits_per_sample.to_string());
                node.metadata
                    .set_property("sample_count".to_string(), f.samples.len().to_string());
            }
            PdfFunction::Type2(f) => {
                node.metadata
                    .set_property("subtype".to_string(), "Exponential".to_string());
                node.metadata
                    .set_property("exponent".to_string(), f.n.to_string());
            }
            PdfFunction::Type3(f) => {
                node.metadata
                    .set_property("subtype".to_string(), "Stitching".to_string());
                node.metadata
                    .set_property("function_count".to_string(), f.functions.len().to_string());
            }
            PdfFunction::Type4(_) => {
                node.metadata
                    .set_property("subtype".to_string(), "PostScript".to_string());
            }
        }

        let node_id = self.ast.add_node(node);

        Some((node_id, function))
    }

    fn parse_type0_function(
        &mut self,
        dict: &PdfDictionary,
        stream: &PdfStream,
    ) -> Option<PdfFunction> {
        let mut func = SampledFunction {
            domain: self.parse_domain(dict)?,
            range: self.parse_range(dict).unwrap_or_default(),
            size: Vec::new(),
            bits_per_sample: 8,
            order: 1,
            encode: Vec::new(),
            decode: Vec::new(),
            samples: Vec::new(),
        };

        // Parse Size array
        if let Some(PdfValue::Array(size)) = dict.get("Size") {
            func.size = size
                .iter()
                .filter_map(|v| v.as_integer().and_then(|i| u32::try_from(i).ok()))
                .collect();
        } else {
            return None;
        }

        // Parse BitsPerSample
        if let Some(bps) = dict.get("BitsPerSample").and_then(|v| v.as_integer()) {
            func.bits_per_sample = match bps {
                1 | 2 | 4 | 8 | 16 | 32 => bps as u32,
                _ => return None,
            };
        }

        // Parse Order (interpolation order)
        if let Some(order) = dict.get("Order").and_then(|v| v.as_integer()) {
            func.order = order as u32;
        }

        // Parse Encode array
        func.encode = self.parse_encode(dict, func.domain.len());

        // Parse Decode array
        func.decode = self.parse_decode(dict, func.range.len());

        // Parse samples from stream
        func.samples = self.parse_samples(stream, &func);

        Some(PdfFunction::Type0(func))
    }

    fn parse_type2_function(&mut self, dict: &PdfDictionary) -> Option<PdfFunction> {
        let mut func = ExponentialFunction {
            domain: self.parse_domain(dict)?,
            range: self.parse_range(dict).unwrap_or_default(),
            c0: vec![0.0],
            c1: vec![1.0],
            n: 1.0,
        };

        // Parse C0 array
        if let Some(PdfValue::Array(c0)) = dict.get("C0") {
            func.c0 = c0.iter().filter_map(|v| self.get_number(v)).collect();
        }

        // Parse C1 array
        if let Some(PdfValue::Array(c1)) = dict.get("C1") {
            func.c1 = c1.iter().filter_map(|v| self.get_number(v)).collect();
        }

        // Parse N (exponent)
        func.n = dict.get("N").and_then(|v| self.get_number(v))?;

        Some(PdfFunction::Type2(func))
    }

    fn parse_type3_function(&mut self, dict: &PdfDictionary) -> Option<PdfFunction> {
        let mut func = StitchingFunction {
            domain: self.parse_domain(dict)?,
            range: self.parse_range(dict).unwrap_or_default(),
            functions: Vec::new(),
            bounds: Vec::new(),
            encode: Vec::new(),
        };

        // Parse Functions array
        if let Some(PdfValue::Array(funcs)) = dict.get("Functions") {
            for f in funcs {
                if let Some((_, parsed_func)) = self.parse_function(f) {
                    func.functions.push(Box::new(parsed_func));
                }
            }
        } else {
            return None;
        }

        // Parse Bounds array
        if let Some(PdfValue::Array(bounds)) = dict.get("Bounds") {
            func.bounds = bounds.iter().filter_map(|v| self.get_number(v)).collect();
        } else {
            return None;
        }

        // Parse Encode array
        if let Some(PdfValue::Array(encode)) = dict.get("Encode") {
            let mut i = 0;
            while i + 1 < encode.len() {
                let min = self.get_number(&encode[i]).unwrap_or(0.0);
                let max = self.get_number(&encode[i + 1]).unwrap_or(1.0);
                func.encode.push((min, max));
                i += 2;
            }
        } else {
            return None;
        }

        Some(PdfFunction::Type3(func))
    }

    fn parse_type4_function(
        &mut self,
        dict: &PdfDictionary,
        stream: &PdfStream,
    ) -> Option<PdfFunction> {
        let func = PostScriptFunction {
            domain: self.parse_domain(dict)?,
            range: self.parse_range(dict)?,
            code: self.extract_postscript_code(stream),
        };

        Some(PdfFunction::Type4(func))
    }

    fn parse_domain(&self, dict: &PdfDictionary) -> Option<Vec<(f64, f64)>> {
        if let Some(PdfValue::Array(domain)) = dict.get("Domain") {
            let mut result = Vec::new();
            let mut i = 0;

            while i + 1 < domain.len() {
                let min = self.get_number(&domain[i])?;
                let max = self.get_number(&domain[i + 1])?;
                result.push((min, max));
                i += 2;
            }

            Some(result)
        } else {
            None
        }
    }

    fn parse_range(&self, dict: &PdfDictionary) -> Option<Vec<(f64, f64)>> {
        if let Some(PdfValue::Array(range)) = dict.get("Range") {
            let mut result = Vec::new();
            let mut i = 0;

            while i + 1 < range.len() {
                let min = self.get_number(&range[i])?;
                let max = self.get_number(&range[i + 1])?;
                result.push((min, max));
                i += 2;
            }

            Some(result)
        } else {
            None
        }
    }

    fn parse_encode(&self, dict: &PdfDictionary, input_dim: usize) -> Vec<(f64, f64)> {
        if let Some(PdfValue::Array(encode)) = dict.get("Encode") {
            let mut result = Vec::new();
            let mut i = 0;

            while i + 1 < encode.len() && result.len() < input_dim {
                let min = self.get_number(&encode[i]).unwrap_or(0.0);
                let max = self.get_number(&encode[i + 1]).unwrap_or(1.0);
                result.push((min, max));
                i += 2;
            }

            result
        } else {
            // Default encode
            (0..input_dim).map(|_| (0.0, 1.0)).collect()
        }
    }

    fn parse_decode(&self, dict: &PdfDictionary, output_dim: usize) -> Vec<(f64, f64)> {
        if let Some(PdfValue::Array(decode)) = dict.get("Decode") {
            let mut result = Vec::new();
            let mut i = 0;

            while i + 1 < decode.len() && result.len() < output_dim {
                let min = self.get_number(&decode[i]).unwrap_or(0.0);
                let max = self.get_number(&decode[i + 1]).unwrap_or(1.0);
                result.push((min, max));
                i += 2;
            }

            result
        } else {
            // Default decode = range
            Vec::new()
        }
    }

    fn parse_samples(&self, stream: &PdfStream, func: &SampledFunction) -> Vec<f64> {
        let data = match stream.decode_with_budget(&self.budget) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };

        let mut samples = Vec::new();
        let bits = func.bits_per_sample as usize;
        let max_val = (1u64 << bits) - 1;

        match bits {
            8 => {
                for byte in data {
                    samples.push(byte as f64 / max_val as f64);
                }
            }
            16 => {
                for chunk in data.chunks(2) {
                    if chunk.len() == 2 {
                        let val = ((chunk[0] as u16) << 8) | (chunk[1] as u16);
                        samples.push(val as f64 / max_val as f64);
                    }
                }
            }
            32 => {
                for chunk in data.chunks(4) {
                    if chunk.len() == 4 {
                        let val = ((chunk[0] as u32) << 24)
                            | ((chunk[1] as u32) << 16)
                            | ((chunk[2] as u32) << 8)
                            | (chunk[3] as u32);
                        samples.push(val as f64 / max_val as f64);
                    }
                }
            }
            _ => {
                // Handle arbitrary bit depths
                let mut bit_reader = BitReader::new(&data);
                while let Some(val) = bit_reader.read_bits(bits) {
                    samples.push(val as f64 / max_val as f64);
                }
            }
        }

        samples
    }

    fn extract_postscript_code(&self, stream: &PdfStream) -> String {
        match stream.decode_with_budget(&self.budget) {
            Ok(data) => String::from_utf8_lossy(&data).to_string(),
            Err(_) => String::new(),
        }
    }

    fn get_number(&self, value: &PdfValue) -> Option<f64> {
        match value {
            PdfValue::Integer(i) => Some(*i as f64),
            PdfValue::Real(r) => Some(*r),
            _ => None,
        }
    }

    /// Evaluate function at given input
    pub fn evaluate(&self, func: &PdfFunction, input: &[f64]) -> Vec<f64> {
        match func {
            PdfFunction::Type0(f) => self.evaluate_type0(f, input),
            PdfFunction::Type2(f) => self.evaluate_type2(f, input),
            PdfFunction::Type3(f) => self.evaluate_type3(f, input),
            PdfFunction::Type4(f) => self.evaluate_type4(f, input),
        }
    }

    fn evaluate_type0(&self, func: &SampledFunction, input: &[f64]) -> Vec<f64> {
        // Simplified evaluation - would need full interpolation
        let mut output = Vec::new();

        // Map input through encode
        let mut encoded = Vec::new();
        for (i, &x) in input.iter().enumerate() {
            if let (Some(&(e_min, e_max)), Some(&(d_min, d_max))) =
                (func.encode.get(i), func.domain.get(i))
            {
                let t = (x - d_min) / (d_max - d_min);
                encoded.push(e_min + t * (e_max - e_min));
            }
        }

        // Sample lookup (simplified - real implementation needs n-dimensional interpolation)
        let sample_index = 0; // Would calculate from encoded values
        let n_outputs = func.range.len();

        for i in 0..n_outputs {
            if sample_index + i < func.samples.len() {
                let sample = func.samples[sample_index + i];

                // Map through decode
                if i < func.decode.len() {
                    let (dec_min, dec_max) = func.decode[i];
                    output.push(dec_min + sample * (dec_max - dec_min));
                } else if i < func.range.len() {
                    let (r_min, r_max) = func.range[i];
                    output.push(r_min + sample * (r_max - r_min));
                } else {
                    output.push(sample);
                }
            }
        }

        output
    }

    fn evaluate_type2(&self, func: &ExponentialFunction, input: &[f64]) -> Vec<f64> {
        if input.is_empty() {
            return Vec::new();
        }

        let x = input[0];
        let mut output = Vec::new();

        // Interpolation formula: f(x) = C0 + x^N * (C1 - C0)
        let n_outputs = func.c0.len().max(func.c1.len());

        for i in 0..n_outputs {
            let c0 = func.c0.get(i).copied().unwrap_or(0.0);
            let c1 = func.c1.get(i).copied().unwrap_or(1.0);

            let result = c0 + x.powf(func.n) * (c1 - c0);

            // Clip to range
            if i < func.range.len() {
                let (r_min, r_max) = func.range[i];
                output.push(result.max(r_min).min(r_max));
            } else {
                output.push(result);
            }
        }

        output
    }

    fn evaluate_type3(&self, func: &StitchingFunction, input: &[f64]) -> Vec<f64> {
        if input.is_empty() || func.functions.is_empty() {
            return Vec::new();
        }

        let x = input[0];
        let Some(&(d_min, d_max)) = func.domain.first() else {
            return Vec::new();
        };
        let x_clipped = x.max(d_min).min(d_max);

        // Find which sub-function to use
        let mut k = 0;
        for (i, &bound) in func.bounds.iter().enumerate() {
            if x_clipped <= bound {
                k = i;
                break;
            }
            k = i + 1;
        }

        if k >= func.functions.len() {
            k = func.functions.len() - 1;
        }

        // Map input through encode for this sub-function
        if k < func.encode.len() {
            let (e_min, e_max) = func.encode[k];

            // Determine bounds for this segment
            let seg_min = if k == 0 { d_min } else { func.bounds[k - 1] };
            let seg_max = if k < func.bounds.len() {
                func.bounds[k]
            } else {
                d_max
            };

            let t = (x_clipped - seg_min) / (seg_max - seg_min);
            let encoded_x = e_min + t * (e_max - e_min);

            // Evaluate sub-function
            self.evaluate(&func.functions[k], &[encoded_x])
        } else {
            self.evaluate(&func.functions[k], &[x_clipped])
        }
    }

    fn evaluate_type4(&self, func: &PostScriptFunction, _input: &[f64]) -> Vec<f64> {
        // PostScript calculator function evaluation
        // This would require a PostScript interpreter
        // For now, return default range values
        func.range.iter().map(|(min, _)| *min).collect()
    }
}

/// Bit reader for arbitrary bit depths
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bits(&mut self, n_bits: usize) -> Option<u32> {
        if self.byte_pos >= self.data.len() {
            return None;
        }

        let mut result = 0u32;
        let mut bits_read = 0;

        while bits_read < n_bits && self.byte_pos < self.data.len() {
            let bits_available = 8 - self.bit_pos;
            let bits_to_read = (n_bits - bits_read).min(bits_available);

            let mask = ((1 << bits_to_read) - 1) as u8;
            let shift = bits_available - bits_to_read;
            let bits = (self.data[self.byte_pos] >> shift) & mask;

            result = (result << bits_to_read) | (bits as u32);
            bits_read += bits_to_read;

            self.bit_pos += bits_to_read;
            if self.bit_pos >= 8 {
                self.bit_pos = 0;
                self.byte_pos += 1;
            }
        }

        if bits_read == n_bits {
            Some(result)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExponentialFunction, FunctionParser, PdfFunction, SampledFunction, StitchingFunction,
    };
    use crate::ast::PdfAstGraph;
    use crate::parser::reference_resolver::ObjectNodeMap;
    use crate::performance::ResourceBudget;
    use crate::types::{PdfArray, PdfDictionary, PdfValue};

    #[test]
    fn type0_evaluation_handles_mismatched_domain_and_encode_arrays() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let parser = FunctionParser::new(&mut ast, &resolver);
        let function = PdfFunction::Type0(SampledFunction {
            domain: Vec::new(),
            range: vec![(0.0, 1.0)],
            size: vec![1],
            bits_per_sample: 8,
            order: 1,
            encode: vec![(0.0, 1.0)],
            decode: vec![(0.0, 1.0)],
            samples: vec![0.5],
        });

        assert_eq!(parser.evaluate(&function, &[0.5]).len(), 1);
    }

    #[test]
    fn type3_evaluation_rejects_empty_domain() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let parser = FunctionParser::new(&mut ast, &resolver);
        let function = PdfFunction::Type3(StitchingFunction {
            domain: Vec::new(),
            range: Vec::new(),
            functions: vec![Box::new(PdfFunction::Type2(ExponentialFunction {
                domain: vec![(0.0, 1.0)],
                range: Vec::new(),
                c0: vec![0.0],
                c1: vec![1.0],
                n: 1.0,
            }))],
            bounds: Vec::new(),
            encode: Vec::new(),
        });

        assert!(parser.evaluate(&function, &[0.5]).is_empty());
    }

    #[test]
    fn function_parser_respects_node_budget() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 0, 10, 8);
        let mut parser = FunctionParser::new_with_budget(&mut ast, &resolver, &budget);
        let mut dict = PdfDictionary::new();
        dict.insert("FunctionType", PdfValue::Integer(2));
        dict.insert(
            "Domain",
            PdfValue::Array(PdfArray::from(vec![
                PdfValue::Integer(0),
                PdfValue::Integer(1),
            ])),
        );
        dict.insert(
            "C0",
            PdfValue::Array(PdfArray::from(vec![PdfValue::Integer(0)])),
        );
        dict.insert(
            "C1",
            PdfValue::Array(PdfArray::from(vec![PdfValue::Integer(1)])),
        );
        dict.insert("N", PdfValue::Integer(1));

        assert!(parser.parse_function(&PdfValue::Dictionary(dict)).is_none());
        assert_eq!(ast.node_count(), 0);
    }
}
