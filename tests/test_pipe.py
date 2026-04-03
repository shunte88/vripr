"""Tests for AudacityPipe — exercises the pipe wrapper without a live Audacity."""
import pytest
from unittest.mock import MagicMock, patch, mock_open
from vripr.app import AudacityPipe


class TestAudacityPipe:
    def test_connect_failure_returns_false(self):
        pipe = AudacityPipe()
        # No Audacity running — pipe files won't exist
        result = pipe.connect()
        assert result is False
        assert pipe.connected is False

    def test_send_when_not_connected_returns_empty(self):
        pipe = AudacityPipe()
        assert pipe.send("GetInfo: Type=Tracks") == ""

    def test_send_reads_until_sentinel(self):
        pipe = AudacityPipe()
        pipe.connected = True

        mock_to   = MagicMock()
        mock_from = MagicMock()
        mock_from.readline.side_effect = [
            '{"key": "value"}\n',
            "BatchCommand finished: OK\n",
        ]
        pipe._to    = mock_to
        pipe._from_ = mock_from

        result = pipe.send("GetInfo: Type=Labels Format=JSON")
        assert result == '{"key": "value"}'
        mock_to.write.assert_called_once()
        mock_to.flush.assert_called_once()

    def test_send_handles_failed_sentinel(self):
        pipe = AudacityPipe()
        pipe.connected = True

        mock_to   = MagicMock()
        mock_from = MagicMock()
        mock_from.readline.side_effect = [
            "error line\n",
            "BatchCommand finished: Failed\n",
        ]
        pipe._to    = mock_to
        pipe._from_ = mock_from

        result = pipe.send("SomeBadCommand:")
        assert result == "error line"

    def test_close_disconnects(self):
        pipe = AudacityPipe()
        pipe.connected = True
        pipe._to    = MagicMock()
        pipe._from_ = MagicMock()
        pipe.close()
        assert pipe.connected is False
        pipe._to.close.assert_called_once()
        pipe._from_.close.assert_called_once()
