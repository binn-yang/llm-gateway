# Documentation

This directory contains comprehensive documentation for the LLM Gateway project.

## Core Documentation

### [IMPLEMENTATION.md](./IMPLEMENTATION.md)
Complete implementation summary covering all phases, features, architecture, and technical details.

**Contents**:
- Project completion status and statistics
- All implementation phases (1-12)
- Feature matrix across providers
- Technical architecture and components
- Load balancing and high availability
- Protocol conversion details
- Performance characteristics
- Testing coverage
- Usage examples and deployment guides

### [FEATURES.md](./FEATURES.md)
Comprehensive feature documentation for all supported capabilities.

**Contents**:
- Text completion
- Streaming support
- Vision/image handling
- Tool/function calling
- Prompt caching (Anthropic)
- Structured outputs (JSON mode, JSON schema)
- Provider-specific features
- Detailed usage examples

### [CONVERSION_LIMITATIONS.md](./CONVERSION_LIMITATIONS.md)
Detailed documentation of protocol conversion trade-offs and limitations.

**Contents**:
- What gets lost or approximated during conversion
- Provider-specific workarounds
- Parameter compatibility matrix
- Best practices for optimal compatibility
- Known limitations and alternatives

### [DAEMON.md](./DAEMON.md)
Guide for running LLM Gateway as a daemon/background service.

**Contents**:
- Recommended deployment approaches
- Process manager options (systemd, PM2, Docker)
- Built-in daemon mode (development only)
- Production deployment best practices

## Archive

The `archive/` directory contains historical documentation:

- `CODE_REVIEW.md` - Initial code review report
- `PROJECT_ANALYSIS.md` - Deep project analysis report
- `SESSION_SUMMARY.md` - Feature implementation session summary
- `STATS_ENHANCEMENT_SUMMARY.md` - Stats command enhancement summary
- `STATS_TESTING_REPORT.md` - Stats testing verification report
- `THINKING_FIELD_FIX.md` - Anthropic thinking field fix documentation
- `IMPLEMENTATION_SUMMARY.md` - Original implementation summary (merged into IMPLEMENTATION.md)
- `PHASES_COMPLETE.md` - Original phases completion report (merged into IMPLEMENTATION.md)

## Quick Navigation

**Getting Started**:
1. Start with the main [README.md](../README.md)
2. Review [FEATURES.md](./FEATURES.md) for capabilities
3. Check [CONVERSION_LIMITATIONS.md](./CONVERSION_LIMITATIONS.md) for provider compatibility

**Development**:
1. Read [IMPLEMENTATION.md](./IMPLEMENTATION.md) for architecture details
2. Refer to [CLAUDE.md](../CLAUDE.md) for development guidelines

**Deployment**:
1. Follow [README.md](../README.md) quick start
2. Review [DAEMON.md](./DAEMON.md) for production deployment
3. Configure monitoring using Prometheus metrics

## External References

- [Anthropic API Documentation](https://docs.anthropic.com/)
- [OpenAI API Documentation](https://platform.openai.com/docs/)
- [Google Gemini API Documentation](https://ai.google.dev/docs)
- [Prometheus Metrics](https://prometheus.io/docs/)
