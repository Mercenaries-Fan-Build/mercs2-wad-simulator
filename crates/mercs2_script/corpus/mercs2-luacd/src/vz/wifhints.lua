import("MrxFactionManager")
import("WifFreePlay")

function HasHint(sSpeaker)
  local tActiveHints = _tActiveHints[sSpeaker]
  if tActiveHints and 0 < #tActiveHints then
    for i, sHint in ipairs(tActiveHints) do
      local tHintData = _tHints[sSpeaker][sHint]
      if _TestHintConstraints(tHintData) then
        return true
      end
    end
  end
  return false
end

function GetHint(sSpeaker)
  local tActiveHints = _tActiveHints[sSpeaker]
  if not tActiveHints then
    return
  end
  local nActiveHints = #tActiveHints
  if nActiveHints <= 0 then
    return
  end
  local nIndex
  if _tLastPlayed[sSpeaker] then
    nIndex = _tLastPlayed[sSpeaker] + 1
  else
    nIndex = 1
  end
  local bValid = false
  for i = 1, nActiveHints do
    if not tActiveHints[nIndex] then
      nIndex = 1
    end
    local sHint = tActiveHints[nIndex]
    local tHintData = _tHints[sSpeaker][sHint]
    bValid = _TestHintConstraints(tHintData)
    if bValid then
      _tLastPlayed[sSpeaker] = nIndex
      return tHintData.sCue
    else
      nIndex = nIndex + 1
    end
  end
end

function _TestHintConstraints(tHintData)
  local bValid = true
  if tHintData.tFactionAttitudeConstraint then
    bValid = MrxFactionManager.TestAttitude(tHintData.tFactionAttitudeConstraint[1], tHintData.tFactionAttitudeConstraint[2], tHintData.tFactionAttitudeConstraint[3], tHintData.tFactionAttitudeConstraint[4])
  end
  return bValid
end

function Reset()
  _tLastPlayed = {}
end

function SaveSingleton()
  return _tActiveHints
end

function LoadSingleton(tSavedActiveHints)
  if type(tSavedActiveHints) ~= "table" then
    return
  end
  _tActiveHints = {}
  _tLastPlayed = {}
  for sSpeaker, tHints in pairs(tSavedActiveHints) do
    if type(tHints) == "table" then
      for _, sHint in ipairs(tHints) do
        AddActiveHint(sHint)
      end
    end
  end
end

function AddActiveHint(sCurrentHint)
  local sSpeaker = _FindSpeaker(sCurrentHint)
  if not sSpeaker then
    return
  end
  _tActiveHints[sSpeaker] = _tActiveHints[sSpeaker] or {}
  table.insert(_tActiveHints[sSpeaker], sCurrentHint)
  WifFreePlay.StartNag()
end

function RemoveActiveHint(sCurrentHint)
  local sSpeaker = _FindSpeaker(sCurrentHint)
  if not sSpeaker then
    return
  end
  if not _tActiveHints[sSpeaker] then
    return
  end
  for nIndex, sHint in ipairs(_tActiveHints[sSpeaker]) do
    if sHint == sCurrentHint then
      table.remove(_tActiveHints[sSpeaker], nIndex)
      if table.getn(_tActiveHints[sSpeaker]) == 0 then
        _tActiveHints[sSpeaker] = nil
      end
      break
    end
  end
end

function _FindSpeaker(sHint)
  if sHint == nil then
    return
  end
  for sSpeaker, tHints in pairs(_tHints) do
    if tHints[sHint] then
      return sSpeaker
    end
  end
end

function UnlockAllHints(sSpeaker)
  if not _tHints[sSpeaker] then
    return
  end
  for sHintName, _ in pairs(_tHints[sSpeaker]) do
    AddActiveHint(sHintName)
  end
end

_tActiveHints = {}
_tLastPlayed = {}
_tHints = {
  Fiona = {
    FionaHint01 = {
      sCue = "Fiona.Hints.01",
      tFactionAttitudeConstraint = {
        "All",
        "Pmc",
        "<=",
        "Hostile"
      }
    },
    FionaHint02 = {
      sCue = "Fiona.Hints.02"
    },
    FionaHint03 = {
      sCue = "Fiona.Hints.03",
      tFactionAttitudeConstraint = {
        "All",
        "Pmc",
        "<=",
        "Hostile"
      }
    },
    FionaHint04 = {
      sCue = "Fiona.Hints.04"
    },
    FionaHint05 = {
      sCue = "Fiona.Hints.05",
      tFactionAttitudeConstraint = {
        "Chi",
        "Pmc",
        "<=",
        "Hostile"
      }
    },
    FionaHint06 = {
      sCue = "Fiona.Hints.06"
    },
    FionaHint07 = {
      sCue = "Fiona.Hints.07"
    },
    FionaHint08 = {
      sCue = "Fiona.Hints.08",
      tFactionAttitudeConstraint = {
        "Chi",
        "Pmc",
        "<=",
        "Hostile"
      }
    },
    FionaHint09 = {
      sCue = "Fiona.Hints.09"
    },
    FionaHint10 = {
      sCue = "Fiona.Hints.10"
    },
    FionaHint11 = {
      sCue = "Fiona.Hints.11",
      tFactionAttitudeConstraint = {
        "Gur",
        "Pmc",
        "<=",
        "Hostile"
      }
    },
    FionaHint12 = {
      sCue = "Fiona.Hints.12"
    },
    FionaHint13 = {
      sCue = "Fiona.Hints.13",
      tFactionAttitudeConstraint = {
        "Gur",
        "Pmc",
        ">=",
        "Friendly"
      }
    },
    FionaHint14 = {
      sCue = "Fiona.Hints.14",
      tFactionAttitudeConstraint = {
        "Gur",
        "Pmc",
        ">=",
        "Friendly"
      }
    },
    FionaHint15 = {
      sCue = "Fiona.Hints.15",
      tFactionAttitudeConstraint = {
        "Gur",
        "Pmc",
        "<=",
        "Hostile"
      }
    },
    FionaHint16 = {
      sCue = "Fiona.Hints.16"
    },
    FionaHint17 = {
      sCue = "Fiona.Hints.17"
    },
    FionaHint18 = {
      sCue = "Fiona.Hints.18"
    },
    FionaHint19 = {
      sCue = "Fiona.Hints.19"
    },
    FionaHint20 = {
      sCue = "Fiona.Hints.20"
    },
    FionaHint21 = {
      sCue = "Fiona.Hints.21"
    },
    FionaHint22 = {
      sCue = "Fiona.Hints.22",
      tFactionAttitudeConstraint = {
        "Oil",
        "Pmc",
        "<=",
        "Hostile"
      }
    },
    FionaHint23 = {
      sCue = "Fiona.Hints.23",
      tFactionAttitudeConstraint = {
        "Oil",
        "Pmc",
        ">=",
        "Friendly"
      }
    },
    FionaHint24 = {
      sCue = "Fiona.Hints.24",
      tFactionAttitudeConstraint = {
        "Oil",
        "Pmc",
        "<=",
        "Hostile"
      }
    },
    FionaHint25 = {
      sCue = "Fiona.Hints.25"
    },
    FionaHint26 = {
      sCue = "Fiona.Hints.26"
    },
    FionaHint27 = {
      sCue = "Fiona.Hints.27"
    },
    FionaHint28 = {
      sCue = "Fiona.Hints.28"
    },
    FionaHint29 = {
      sCue = "Fiona.Hints.29"
    },
    FionaHint30 = {
      sCue = "Fiona.Hints.30"
    },
    FionaHint31 = {
      sCue = "Fiona.Hints.31"
    },
    FionaHint32 = {
      sCue = "Fiona.Hints.32"
    },
    FionaHint33 = {
      sCue = "Fiona.Hints.33"
    }
  },
  Ewan = {
    EwanHint01 = {
      sCue = "Ewan.Misc.Hints01"
    },
    EwanHint02 = {
      sCue = "Ewan.Misc.Hints02"
    },
    EwanHint03 = {
      sCue = "Ewan.Misc.Hints03"
    },
    EwanHint04 = {
      sCue = "Ewan.Misc.Hints04"
    },
    EwanHint05 = {
      sCue = "Ewan.Misc.Hints05"
    },
    EwanHint06 = {
      sCue = "Ewan.Misc.Hints06"
    }
  },
  Eva = {
    EvaHint01 = {
      sCue = "Eva.Misc.Hint01"
    },
    EvaHint02 = {
      sCue = "Eva.Misc.Hint02"
    },
    EvaHint03 = {
      sCue = "Eva.Misc.Hint03"
    },
    EvaHint04 = {
      sCue = "Eva.Misc.Hint04"
    },
    EvaHint05 = {
      sCue = "Eva.Misc.Hint05"
    },
    EvaHint06 = {
      sCue = "Eva.Misc.Hint06"
    },
    EvaHint08 = {
      sCue = "Eva.Misc.Hint08"
    }
  },
  Misha = {
    MishaHint01 = {
      sCue = "Misha.Misc.Hint01"
    },
    MishaHint02 = {
      sCue = "Misha.Misc.Hint02"
    },
    MishaHint03 = {
      sCue = "Misha.Misc.Hint03"
    },
    MishaHint04 = {
      sCue = "Misha.Misc.Hint04"
    },
    MishaHint05 = {
      sCue = "Misha.Misc.Hint05"
    },
    MishaHint06 = {
      sCue = "Misha.Misc.Hint06"
    },
    MishaHint07 = {
      sCue = "Misha.Misc.Hint07"
    },
    MishaHint08 = {
      sCue = "Misha.Misc.Hint08"
    },
    MishaHint09 = {
      sCue = "Misha.Misc.Hint09"
    },
    MishaHint10 = {
      sCue = "Misha.Misc.Hint10"
    },
    MishaHint11 = {
      sCue = "Misha.Misc.Hint11"
    },
    MishaHint12 = {
      sCue = "Misha.Misc.Hint12"
    },
    MishaHint13 = {
      sCue = "Misha.Misc.Hint13"
    },
    MishaHint14 = {
      sCue = "Misha.Misc.Hint14"
    },
    MishaHint15 = {
      sCue = "Misha.Misc.Hint15"
    },
    MishaHint16 = {
      sCue = "Misha.Misc.Hint16"
    }
  }
}
