"""
Unit tests for Surveyor Agent.
"""

import pytest
import json
from casparian_flow.agents.surveyor import SurveyorAgent, PhaseResult
from casparian_flow.db.models import (
    SurveyorSession,
    PhaseEnum,
    FileVersion,
    FileLocation,
    SourceRoot,
    LibraryWhitelist,
)
from casparian_flow.db.setup import seed_library_whitelist


@pytest.fixture
def surveyor_agent(test_db_session, test_db_engine):
    """Create a SurveyorAgent with mock dependencies."""
    from casparian_flow.services.scout import Scout
    from casparian_flow.services.architect import ArchitectService
    from casparian_flow.services.ai_hook import MockGenerator
    from casparian_flow.services.test_generator import TestGenerator

    # Seed library whitelist
    seed_library_whitelist(test_db_engine)

    scout = Scout(test_db_session)
    architect = ArchitectService(test_db_engine, "test-secret-key")
    llm_generator = MockGenerator()
    test_generator = TestGenerator(llm_generator)

    return SurveyorAgent(
        db_session=test_db_session,
        scout=scout,
        architect=architect,
        llm_generator=llm_generator,
        test_generator=test_generator,
    )


@pytest.fixture
def test_surveyor_session(test_db_session, test_source_root):
    """Create a test surveyor session."""
    session = SurveyorSession(
        source_root_id=test_source_root,
        current_phase=PhaseEnum.IDLE,
        phase_data=json.dumps({}),
    )
    test_db_session.add(session)
    test_db_session.commit()
    test_db_session.refresh(session)
    return session


def test_create_session(surveyor_agent, test_source_root):
    """Test creating a new surveyor session."""
    session = surveyor_agent.create_session(test_source_root)

    assert session.id is not None
    assert session.source_root_id == test_source_root
    assert session.current_phase == PhaseEnum.IDLE
    assert session.phase_data == "{}"
    assert session.completed_at is None


def test_log_decision(surveyor_agent, test_surveyor_session):
    """Test logging a decision."""
    from casparian_flow.db.models import SurveyorDecision

    surveyor_agent.log_decision(
        session=test_surveyor_session,
        phase=PhaseEnum.PHASE_1_RECONNAISSANCE,
        decision_type="test_decision",
        data={"key": "value"},
        reasoning="Test reasoning",
    )

    # Verify decision was logged
    decision = surveyor_agent.db.query(SurveyorDecision).filter_by(
        session_id=test_surveyor_session.id
    ).first()

    assert decision is not None
    assert decision.phase == PhaseEnum.PHASE_1_RECONNAISSANCE
    assert decision.decision_type == "test_decision"
    assert json.loads(decision.decision_data) == {"key": "value"}
    assert decision.reasoning == "Test reasoning"


def test_advance_phase(surveyor_agent, test_surveyor_session):
    """Test advancing to next phase."""
    surveyor_agent.advance_phase(test_surveyor_session, PhaseEnum.PHASE_1_RECONNAISSANCE)

    assert test_surveyor_session.current_phase == PhaseEnum.PHASE_1_RECONNAISSANCE


def test_phase_1_no_files(surveyor_agent, test_surveyor_session):
    """Test Phase 1 when no unprocessed files exist."""
    result = surveyor_agent.phase_1_reconnaissance(test_surveyor_session)

    assert result.success is True
    assert result.next_phase == PhaseEnum.COMPLETED
    assert result.data["unprocessed_count"] == 0


def test_phase_2_environment_check(surveyor_agent, test_surveyor_session):
    """Test Phase 2 environment check."""
    result = surveyor_agent.phase_2_environment_check(test_surveyor_session)

    assert result.success is True
    assert result.next_phase == PhaseEnum.PHASE_3_CONSTRUCTION
    assert "libraries" in result.data
    assert len(result.data["libraries"]) > 0  # Should have seeded libraries


def test_execute_phase_idle(surveyor_agent, test_surveyor_session):
    """Test executing phase starting from IDLE."""
    # Starting from IDLE should trigger Phase 1
    result = surveyor_agent.execute_phase(test_surveyor_session)

    # Since there are no files, Phase 1 should complete and mark session as completed
    assert result.success is True
    assert result.next_phase == PhaseEnum.COMPLETED
