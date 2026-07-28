NETEVENT_ENTERFREEPLAY = 0
NETEVENT_ENTERCONTRACT = 1
NETEVENT_PLAYSPECIALMUSIC = 2
NETEVENT_STOPSPECIALMUSIC = 3
_bPrevDynamic = true

function _DisableDynamicMusic()
  _bPrevDynamic = Sound.IsDynamicMusic()
  Sound.SetDynamicMusic(false)
end

function _RestoreDynamicMusic()
  Sound.SetDynamicMusic(_bPrevDynamic)
end

_tMusicCues = {
  factions = {
    an = {
      explore = {
        "mu_fac_an_explore_01"
      },
      action = {
        "mu_fac_an_threat_01"
      },
      mission_success = {
        "mu_fac_an_win_01"
      },
      mission_failure = {
        "mu_fac_an_fail_01"
      },
      hijack = {
        "mu_fac_an_hijack_01",
        "mu_fac_an_hijack_02",
        "mu_fac_an_hijack_03"
      },
      hijack_success = {
        "mu_fac_an_kickass_01"
      },
      shell = {
        "mu_shell_01"
      },
      pause = {
        "mu_shell_01"
      }
    },
    oc = {
      explore = {
        "mu_fac_oc_explore_01"
      },
      action = {
        "mu_fac_oc_threat_01"
      },
      mission_success = {
        "mu_fac_oc_win_01"
      },
      mission_failure = {
        "mu_fac_oc_fail_01"
      },
      hijack = {
        "mu_fac_oc_hijack_01",
        "mu_fac_oc_hijack_02"
      },
      hijack_success = {
        "mu_fac_oc_kickass_01"
      },
      shell = {
        "mu_shell_01"
      },
      pause = {
        "mu_shell_01"
      }
    },
    gr = {
      explore = {
        "mu_fac_gr_explore_01"
      },
      action = {
        "mu_fac_gr_threat_01"
      },
      mission_success = {
        "mu_fac_gr_win_01"
      },
      mission_failure = {
        "mu_fac_gr_fail_01"
      },
      hijack = {
        "mu_fac_gr_hijack_01",
        "mu_fac_gr_hijack_02",
        "mu_fac_gr_hijack_03"
      },
      hijack_success = {
        "mu_fac_gr_kickass_01"
      },
      shell = {
        "mu_shell_01"
      },
      pause = {
        "mu_shell_01"
      }
    },
    ch = {
      explore = {
        "mu_fac_ch_explore_01"
      },
      action = {
        "mu_fac_ch_threat_01"
      },
      mission_success = {
        "mu_fac_ch_win_01"
      },
      mission_failure = {
        "mu_fac_ch_fail_01"
      },
      hijack = {
        "mu_fac_ch_hijack_01",
        "mu_fac_ch_hijack_02",
        "mu_fac_ch_hijack_03"
      },
      hijack_success = {
        "mu_fac_ch_kickass_01"
      },
      shell = {
        "mu_shell_01"
      },
      pause = {
        "mu_shell_01"
      }
    },
    pmc = {
      explore = {
        "mu_fac_pmc_explore_01"
      },
      action = {
        "mu_fac_pmc_threat_01"
      },
      mission_success = {
        "mu_fac_pmc_win_01"
      },
      mission_failure = {
        "mu_fac_pmc_fail_01"
      },
      hijack = {
        "mu_fac_oc_hijack_01",
        "mu_fac_oc_hijack_02"
      },
      hijack_success = {
        "mu_fac_pmc_kickass_01"
      },
      shell = {
        "mu_shell_01"
      },
      pause = {
        "mu_shell_01"
      }
    }
  },
  freeplay = {
    freeplay_city = {
      explore = {
        "mu_nomission_city_explore_01"
      },
      action = {
        "mu_nomission_city_threat_01"
      },
      high_action = {
        "mu_nomission_city_threat_02"
      },
      mission_failure = {
        "mu_nomission_city_fail_01"
      },
      mission_success = {
        "mu_fac_pmc_win_01"
      },
      hijack = {
        "mu_fac_oc_hijack_01",
        "mu_fac_oc_hijack_02"
      },
      hijack_success = {
        "mu_fac_pmc_kickass_01"
      },
      shell = {
        "mu_shell_01"
      },
      pause = {
        "mu_shell_01"
      }
    },
    freeplay_jungle = {
      explore = {
        "mu_nomission_jungle_explore_01"
      },
      action = {
        "mu_nomission_jungle_threat_01"
      },
      high_action = {
        "mu_nomission_jungle_threat_02"
      },
      mission_failure = {
        "mu_nomission_jungle_fail_01"
      },
      mission_success = {
        "mu_fac_pmc_win_01"
      },
      hijack = {
        "mu_fac_oc_hijack_01",
        "mu_fac_oc_hijack_02"
      },
      hijack_success = {
        "mu_fac_pmc_kickass_01"
      },
      shell = {
        "mu_shell_01"
      },
      pause = {
        "mu_shell_01"
      }
    },
    freeplay_water = {
      explore = {
        "mu_nomission_water_explore_01"
      },
      action = {
        "mu_nomission_water_threat_01"
      },
      high_action = {
        "mu_nomission_water_threat_02"
      },
      mission_failure = {
        "mu_nomission_water_fail_01"
      },
      mission_success = {
        "mu_fac_pmc_win_01"
      },
      hijack = {
        "mu_fac_oc_hijack_01",
        "mu_fac_oc_hijack_02"
      },
      hijack_success = {
        "mu_fac_pmc_kickass_01"
      },
      shell = {
        "mu_shell_01"
      },
      pause = {
        "mu_shell_01"
      }
    }
  }
}
_sRootFactionRegion = "freeplay_city"
_sSourceMusicState = "source"
_tSourceMusicTransitions = {
  {entryState = "none", exitState = "none"},
  {entryState = "silence", exitState = "silence"},
  {entryState = "explore", exitState = "explore"}
}
_sHijackSuccessMusicState = "hijack_success"
_sHijackResumeMusicState = "hijack_success_resume"
_fNonActionInterval = 5
_fActionInterval = 15
_tMiscMusicStates = {"misc1", "misc2"}

function SetMusicActionInterval(fActionInterval)
  if fActionInterval < 0 then
    Debug.Printf("Invalid music action interval. Using default " .. tostring(_fActionInterval))
  else
    _fActionInterval = fActionInterval
  end
end

function BindMusicCue(sFaction, sState, iCueIndex, sCue)
  if 0 < iCueIndex and iCueIndex < 4 then
    local bFound = false
    for _, category in pairs(_tMusicCues) do
      for faction, stateTable in pairs(category) do
        if faction == sFaction then
          for state, cueTable in pairs(stateTable) do
            if state == sState then
              cueTable[iCueIndex] = sCue
              bFound = true
            end
          end
        end
      end
    end
    if not bFound then
      Debug.Printf("!!! MUSIC WARNING - Bad cue binding for: " .. sFaction .. ", " .. sState .. ", " .. tostring(iCueIndex) .. ", " .. sCue)
    end
  else
    Debug.Printf("!!! MUSIC WARNING - Cue index out of range for: " .. sFaction .. ", " .. sState .. ", " .. tostring(iCueIndex) .. ", " .. sCue)
  end
end

function _InitializeMusic()
  for faction, stateTable in pairs(_tMusicCues.factions) do
    _InitializeFaction(faction)
    _BindMusicStateCues(faction, stateTable)
  end
  for freeplay, stateTable in pairs(_tMusicCues.freeplay) do
    _InitializeFreeplay(freeplay)
    _BindMusicStateCues(freeplay, stateTable)
  end
  Sound.SetRootFactionRegionMusic(_sRootFactionRegion)
  Sound.SetSourceMusic(_sSourceMusicState)
  if Sound._GetLibVersion() >= 11 then
    Sound.ClearSourceMusicEntryStates()
    for _, transition in pairs(_tSourceMusicTransitions) do
      Sound.AddSourceMusicEntryState(transition.entryState)
    end
  else
    Sound.ClearSourceMusicTransitions()
    for _, transition in pairs(_tSourceMusicTransitions) do
      Sound.SetSourceMusicTransition(transition.entryState, transition.exitState)
    end
  end
  Sound.SetHijackMusic(_sHijackSuccessMusicState, _sHijackResumeMusicState)
  if not _evClientJoined and Net.IsServer() then
    _evClientJoined = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, SendPlayerJoinEvents)
  end
end

function SendPlayerJoinEvents()
  if sFaction then
    Net.SendCustomEvent("MrxMusic", NETEVENT_ENTERCONTRACT, {sFaction}, true)
  else
    Net.SendCustomEvent("MrxMusic", NETEVENT_ENTERFREEPLAY, {}, true)
  end
  if _sCurrentMusicCue then
    Net.SendCustomEvent("MrxMusic", NETEVENT_PLAYSPECIALMUSIC, {_sCurrentMusicCue}, true)
  else
    Net.SendCustomEvent("MrxMusic", NETEVENT_STOPSPECIALMUSIC, {
      _sStopSpecialMusicCue or "silence",
      0
    }, true)
  end
end

function _InitializeFaction(sFaction)
  Sound.AddFactionMusic(sFaction)
  Sound.AddMusicState("none", 15, 0, 0, _fNonActionInterval, 0)
  Sound.AddMusicState("explore", 30, 0, 0, _fNonActionInterval, 0)
  Sound.AddMusicState("action", 0, 3, 0, _fActionInterval, 0)
  Sound.AddMusicState("mission_success", 0, -1, 0, 0, 5)
  Sound.AddMusicState("mission_failure", 0, -1, 0, 0, 5)
  Sound.AddMusicState("hijack", 0, -1, 0, 0, 4)
  Sound.AddMusicState("hijack_success", 120, 3, 0, 10, 8)
  Sound.AddMusicState("hijack_success_resume", 0, -1, 0, 0, 8)
  Sound.AddMusicState("source", 0, 0, 0, _fNonActionInterval, 4)
  Sound.AddMusicState("shell", 0, -1, 0, 0, 4)
  Sound.AddMusicState(_tMiscMusicStates[1], 0, -1, 0, 0, 4)
  Sound.AddMusicState(_tMiscMusicStates[2], 0, -1, 0, 0, 4)
  Sound.AddMusicState("pause", 0, -1, 0.25, 0, 2)
  Sound.AddMusicState("silence", 0, -1, 0, 0, 4)
  Sound.SetActionThresholdsMusic("none", 2, 0)
  Sound.SetActionThresholdsMusic("explore", 2, 0)
  Sound.AddMusicTransition("none", "explore", 1, 1, 0)
  Sound.AddMusicTransition("none", "action", 1, 0, 0)
  Sound.AddMusicTransition("explore", "none", 1, 1, 0)
  Sound.AddMusicTransition("explore", "action", 1, 0, 0)
  Sound.AddMusicTransition("source", "action", 1, 0, 0)
  Sound.AddMusicTransition("action", "explore", 1, 0, 0)
  Sound.AddMusicTransition("hijack_success", "explore", 1, 3, 0)
  Sound.AddMusicTransition("hijack_success", "explore", 1, 0, 0)
  Sound.AddMusicTransition("hijack_success", "hijack_success_resume", 1, 1, 0)
  Sound.AddMusicTransition("hijack_success_resume", "explore", 1, 3, 0)
  Sound.AddMusicTransition("hijack_success_resume", "action", 1, 3, 0)
  Sound.AddMusicTransition("mission_failure", "silence", 1, 2, 0)
  Sound.AddMusicTransition("mission_success", "silence", 1, 2, 0)
end

function _InitializeFreeplay(sFreeplay)
  Sound.AddFactionMusic(sFreeplay)
  Sound.AddMusicState("none", 15, 0, 0, _fNonActionInterval, 0)
  Sound.AddMusicState("explore", 30, 0, 0, _fNonActionInterval, 0)
  Sound.AddMusicState("action", 0, 1, 0, _fActionInterval, 0)
  Sound.AddMusicState("high_action", 0, 2, 0, _fActionInterval, 0)
  Sound.AddMusicState("mission_success", 0, -1, 0, 0, 5)
  Sound.AddMusicState("mission_failure", 0, -1, 0, 0, 5)
  Sound.AddMusicState("hijack", 0, -1, 0, 0, 4)
  Sound.AddMusicState("hijack_success", 120, 3, 0, 10, 8)
  Sound.AddMusicState("hijack_success_resume", 0, -1, 0, 0, 8)
  Sound.AddMusicState("source", 0, 0, 0, _fNonActionInterval, 4)
  Sound.AddMusicState("shell", 0, -1, 0, 0, 4)
  Sound.AddMusicState(_tMiscMusicStates[1], 0, -1, 0, 0, 4)
  Sound.AddMusicState(_tMiscMusicStates[2], 0, -1, 0, 0, 4)
  Sound.AddMusicState("pause", 0, -1, 0.25, 0, 2)
  Sound.AddMusicState("silence", 0, -1, 0, 0, 4)
  Sound.SetActionThresholdsMusic("none", 2, 0)
  Sound.SetActionThresholdsMusic("explore", 2, 0)
  Sound.AddMusicTransition("none", "explore", 1, 1, 0)
  Sound.AddMusicTransition("none", "action", 1, 0, 0)
  Sound.AddMusicTransition("none", "high_action", 1, 0, 0)
  Sound.AddMusicTransition("explore", "none", 1, 1, 0)
  Sound.AddMusicTransition("explore", "action", 1, 0, 0)
  Sound.AddMusicTransition("explore", "high_action", 1, 0, 0)
  Sound.AddMusicTransition("source", "action", 1, 0, 0)
  Sound.AddMusicTransition("source", "high_action", 1, 0, 0)
  Sound.AddMusicTransition("action", "explore", 1, 0, 0)
  Sound.AddMusicTransition("action", "high_action", 1, 0, 0)
  Sound.AddMusicTransition("high_action", "explore", 1, 0, 0)
  Sound.AddMusicTransition("high_action", "action", 1, 0, 0)
  Sound.AddMusicTransition("hijack_success", "explore", 1, 3, 0)
  Sound.AddMusicTransition("hijack_success", "explore", 1, 0, 0)
  Sound.AddMusicTransition("hijack_success", "hijack_success_resume", 1, 1, 0)
  Sound.AddMusicTransition("hijack_success_resume", "explore", 1, 3, 0)
  Sound.AddMusicTransition("hijack_success_resume", "action", 1, 3, 0)
  Sound.AddMusicTransition("hijack_success_resume", "high_action", 1, 3, 0)
  Sound.AddMusicTransition("mission_failure", "silence", 1, 2, 0)
  Sound.AddMusicTransition("mission_success", "silence", 1, 2, 0)
end

function _BindMusicStateCues(sFaction, tCues)
  Sound.SetFactionMusic(sFaction)
  for state, cueTable in pairs(tCues) do
    for _, cue in pairs(cueTable) do
      Sound.BindMusicCue(cue, state)
    end
  end
end

function Reset()
  Sound.SetDynamicMusic(true)
  _bPrevDynamic = true
  _CleanupSpecialMusic()
  _bPrevFactionLock = false
  Sound.LockFactionMusic(false)
  Sound.SetActionLevelsMusic(0, 0, 0, 0)
  Sound.LockActionLevelMusic(false)
end

function EnterFreeplayMusic()
  Debug.Printf("MRXMUSIC ------------ EnterFreeplayMusic()")
  Reset()
  Sound.ActivateFactionRegionMusic()
  Sound.TransitionMusic("explore")
  if Net.IsServer() then
    _sCurrentContractFaction = nil
    Net.SendCustomEvent("MrxMusic", NETEVENT_ENTERFREEPLAY, {}, true)
  end
end

function EnterContractMusic(sFaction)
  Debug.Printf("MRXMUSIC ------------ EnterContractMusic(" .. tostring(sFaction) .. ")")
  Sound.SetFactionMusic(sFaction)
  Sound.LockFactionMusic(true)
  Sound.TransitionMusic("explore")
  if Net.IsServer() then
    _sCurrentContractFaction = sFaction
    Net.SendCustomEvent("MrxMusic", NETEVENT_ENTERCONTRACT, {sFaction}, true)
  end
end

function PlayFanfare(bMissionSuccess)
  _CleanupSpecialMusic()
  if bMissionSuccess then
    Sound.TransitionMusic("mission_success", true)
  else
    Sound.TransitionMusic("mission_failure", true)
  end
end

_bPrevFactionLock = false
_iCurrentMiscMusicIndex = 0
_bPlayingSpecialMusic = false

function PlaySpecialMusic(sMusicCue)
  Debug.Printf("MRXMUSIC ------------ PlaySpecialMusic(" .. tostring(sMusicCue) .. ")")
  if _iCurrentMiscMusicIndex == 0 then
    _bPrevFactionLock = Sound.IsFactionLockedMusic()
  end
  Sound.LockFactionMusic(true)
  _SetMiscMusicIndex()
  Sound.ClearMusicCues(_tMiscMusicStates[_iCurrentMiscMusicIndex])
  Sound.BindMusicCue(sMusicCue, _tMiscMusicStates[_iCurrentMiscMusicIndex])
  Sound.TransitionMusic(_tMiscMusicStates[_iCurrentMiscMusicIndex])
  if Net.IsServer() then
    _sCurrentMusicCue = sMusicCue
    Net.SendCustomEvent("MrxMusic", NETEVENT_PLAYSPECIALMUSIC, {sMusicCue}, true)
  end
  _bPlayingSpecialMusic = true
end

function _SetMiscMusicIndex()
  if _iCurrentMiscMusicIndex > 1 then
    _iCurrentMiscMusicIndex = _iCurrentMiscMusicIndex - 1
  else
    _iCurrentMiscMusicIndex = _iCurrentMiscMusicIndex + 1
  end
end

function _ResumeSpecialMusic()
  if _bPlayingSpecialMusic then
    Sound.TransitionMusic(_tMiscMusicStates[_iCurrentMiscMusicIndex])
  end
end

function _IsPlayingSpecialMusic()
  return _bPlayingSpecialMusic
end

function StopSpecialMusic(sNewState)
  if _bPlayingSpecialMusic then
    Debug.Printf("MRXMUSIC ------------ StopSpecialMusic(" .. tostring(sNewState) .. ")")
    _CleanupSpecialMusic()
    if sNewState then
      Sound.TransitionMusic(sNewState)
    else
      Sound.TransitionMusic("none")
    end
    if Net.IsServer() then
      _sCurrentMusicCue = nil
      _sStopSpecialMusicCue = sNewState
      Net.SendCustomEvent("MrxMusic", NETEVENT_STOPSPECIALMUSIC, {
        sNewState or "none",
        0
      }, true)
    end
  end
end

function _CleanupSpecialMusic()
  if _bPlayingSpecialMusic then
    Sound.LockFactionMusic(_bPrevFactionLock)
    _iCurrentMiscMusicIndex = 0
    _bPlayingSpecialMusic = false
    if Net.IsServer() then
      _sCurrentMusicCue = nil
      Net.SendCustomEvent("MrxMusic", NETEVENT_STOPSPECIALMUSIC, {
        sNewState or "none",
        1
      }, true)
    end
  end
end

function AddMusicPlaylist(sPlaylist, fGap)
  Sound.AddMusicSourcePlaylist(sPlaylist, fGap)
end

function BindPlaylistCue(sPlaylist, sCue)
  Sound.AddCueToMusicSourcePlaylist(sPlaylist, sCue)
end

function ClearMusicPlaylist(sPlaylist)
  Sound.ClearMusicSourcePlaylist(sPlaylist)
end

function GetFactionByStringHash(uFactionStringHash)
  for _, category in pairs(_tMusicCues) do
    for faction, stateTable in pairs(category) do
      if String.GetHash(faction) == uFactionStringHash then
        return faction
      end
    end
  end
  return nil
end

function GetStateByStringHash(uStateStringHash)
  for _, category in pairs(_tMusicCues) do
    for faction, stateTable in pairs(category) do
      for state, cueTable in pairs(stateTable) do
        if String.GetHash(state) == uStateStringHash then
          return state
        end
      end
    end
  end
  return "silence"
end

function NetEventCallback(nEventType, tArgs)
  if nEventType == NETEVENT_ENTERFREEPLAY then
    EnterFreeplayMusic()
  elseif nEventType == NETEVENT_ENTERCONTRACT then
    local sFaction = GetFactionByStringHash(tArgs[1])
    if sFaction then
      EnterContractMusic(sFaction)
    else
      ASSERT(0, "failed to find faction with stringhash " .. tostring(tArgs[1]))
    end
  elseif nEventType == NETEVENT_PLAYSPECIALMUSIC then
    PlaySpecialMusic(tArgs[1])
  elseif nEventType == NETEVENT_STOPSPECIALMUSIC then
    local sNewState = GetStateByStringHash(tArgs[1])
    if tArgs[2] == 1 then
      _CleanupSpecialMusic()
    else
      StopSpecialMusic(sNewState)
    end
  end
end
