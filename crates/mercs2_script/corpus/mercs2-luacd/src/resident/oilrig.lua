import("MrxLayerManager")
_tOilrigEvents = nil

function Init()
  if not _tOilrigEvents then
    _tOilrigEvents = {}
    tTGLayers = {}
    tTGLayers["0x000B637B"] = "vz_state_mer_oilrig_pristine"
    tTGLayers["0x0009878A"] = "vz_state_chijob009_a_pristine"
    tTGLayers["0x00098789"] = "vz_state_chijob009_b_pristine"
  end
end

function Deinit()
  for guid, tEvents in pairs(_tOilrigEvents) do
    _CleanupOilrigEvents(guid)
  end
  _tOilrigEvents = nil
  tTGLayers = nil
end

function OnDeactivate(uGuid, args)
  if _tOilrigEvents[uGuid] then
    _FinishDestruction(uGuid)
  end
  if not Object.IsAlive(uGuid) then
    local sLayer = tTGLayers[Sys.GuidToString(uGuid)]
    MrxLayerManager.Remove(sLayer)
  end
end

function OnStateChange(uiGuid, uiNodeHashName, uiStateHashName)
  local sStateHashName = Sys.GuidToString(uiStateHashName)
  if sStateHashName == "0x28825D4C" then
    local sLayer = tTGLayers[Sys.GuidToString(uiGuid)]
    MrxLayerManager.Remove(sLayer .. "_tg")
    Sound.CueSound(uiGuid, "seq_oilrig_destruction")
    Camera.Shake(StringToGuid("0x1"), "ShakeCameraConstantlyRandom", uiGuid, 0.5, 2000)
    local tEvents = {}
    local e = Event.Create(Event.TimerRelative, {2.5}, _DestroyOilrigSequence, {uiGuid, uiNodeHashName})
    table.insert(tEvents, e)
    _tOilrigEvents[uiGuid] = tEvents
  elseif sStateHashName == "0x694683EB" then
    Sound.CueSound(uiGuid, "sfx_amb_oilrig_destruction")
    Camera.Shake(StringToGuid("0x1"), "ShakeCameraConstantlyRandom", uiGuid, 1.2, 2000)
  end
end

function _CleanupOilrigEvents(uiGuid)
  local tEvents = _tOilrigEvents[uiGuid]
  if tEvents then
    for i, e in pairs(tEvents) do
      Event.Delete(e)
    end
    _tOilrigEvents[uiGuid] = nil
  end
end

function _FinishDestruction(uiGuid)
  Camera.Shake(StringToGuid("0x1"), "StopShakeCameraConstantly", uiGuid)
  _CleanupOilrigEvents(uiGuid)
  Event.Post("oilrigDestroyed", {uiGuid})
end

function _DestroyBuildingA(uOilrig)
  local uBldg = ObjectState.GetLinkGuid(uOilrig, String.GetHash("hp_snap_oilrig_bld_buildingA"))
  if not Object.IsAlive(uBldg) then
    return
  end
  local stateTable = {
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1A_oilrig_towerblowoutsmallA"
      }
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1B_oilrig_cranesmallA"
      },
      minTime = 1,
      maxTime = 1.5
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1B_oilrig_cranelargeA"
      }
    }
  }
  _ProcessNextEvent(uOilrig, stateTable, 1)
end

function _DestroyBuildingB(uOilrig)
  local uBldg = ObjectState.GetLinkGuid(uOilrig, String.GetHash("hp_snap_oilrig_bld_buildingB"))
  if not Object.IsAlive(uBldg) then
    return
  end
  local stateTable = {
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1A_oilrig_towerblowoutsmallA"
      },
      minTime = 0.7,
      maxTime = 0.9
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1B_oilrig_radiojammer"
      },
      minTime = 0.5
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_mv_piece1B_oilrig_helipadsmallA"
      },
      minTime = 1.4,
      maxTime = 1.6
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece2A_oilrig_cranesmallA"
      }
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece2B_oilrig_cranelargeA"
      }
    }
  }
  _ProcessNextEvent(uOilrig, stateTable, 1)
end

function _DestroyBuildingC(uOilrig)
  local uBldg = ObjectState.GetLinkGuid(uOilrig, String.GetHash("hp_snap_oilrig_bld_buildingC"))
  if not Object.IsAlive(uBldg) then
    return
  end
  local stateTable = {
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1C_oilrig_cranesmallA"
      },
      minTime = 1
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1C_oilrig_towerblowoutlargeA"
      },
      minTime = 2.5
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1B_oilrig_cranelargeA"
      }
    }
  }
  _ProcessNextEvent(uOilrig, stateTable, 1)
end

function _DestroyBuildingD(uOilrig)
  local uBldg = ObjectState.GetLinkGuid(uOilrig, String.GetHash("hp_snap_oilrig_bld_buildingD"))
  if not Object.IsAlive(uBldg) then
    return
  end
  local stateTable = {
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1b_oilrig_tankmedA_a"
      },
      minTime = 0.25
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1b_oilrig_tankmedA_b"
      },
      minTime = 0.25
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1C_oilrig_tankmedA_c"
      },
      minTime = 0.25
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1b_oilrig_att_pipeblowout"
      },
      minTime = 0.75
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_mv_piece1b_oilrig_smokestack"
      },
      minTime = 1
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_mv_piece1A_oilrig_bld_helipadsmallA"
      },
      minTime = 0.2
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uBldg,
        "hp_snap_piece1A_oilrig_cranelargeA"
      }
    }
  }
  _ProcessNextEvent(uOilrig, stateTable, 1)
end

function _DestroyOilrigSequence(uOilrig, uNodeHashName)
  uLargeExplosion = String.GetHash("fx_Explosion_HugeOil_RigOnly")
  uHugeExplosion = String.GetHash("fx_Explosion_HugeOilTower_RigOnly")
  local dataTable = {
    {
      fn = ObjectState.StartEmitter,
      args = {
        uOilrig,
        String.GetHash("hp_fx_explosionA"),
        uLargeExplosion
      },
      minTime = 1,
      maxTime = 1.2
    },
    {
      fn = ObjectState.StartEmitter,
      args = {
        uOilrig,
        String.GetHash("hp_fx_explosionD"),
        uLargeExplosion
      },
      minTime = 0.5
    },
    {
      fn = ObjectState.StartEmitter,
      args = {
        uOilrig,
        String.GetHash("hp_fx_explosionE"),
        uHugeExplosion
      },
      minTime = 0.8,
      maxTime = 1
    },
    {
      fn = ObjectState.StartEmitter,
      args = {
        uOilrig,
        String.GetHash("hp_fx_explosionF"),
        uHugeExplosion
      },
      minTime = 0.8,
      maxTime = 1
    },
    {
      fn = _DestroyBuildingD,
      args = {uOilrig},
      minTime = 1
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_catwalkB"
      }
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_scaffold"
      },
      minTime = 0.5
    },
    {
      fn = ObjectState.SetState,
      args = {
        uOilrig,
        uNodeHashName,
        String.GetHash("CollapseState")
      },
      minTime = 4,
      maxTime = 4.5
    },
    {
      fn = _DestroyBuildingA,
      args = {uOilrig},
      minTime = 2,
      maxTime = 2.5
    },
    {
      fn = ObjectState.StartEmitter,
      args = {
        uOilrig,
        String.GetHash("hp_fx_explosionI"),
        uHugeExplosion
      },
      minTime = 0.5
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_towerblowoutdiagonal"
      },
      minTime = 2,
      maxTime = 2.5
    },
    {
      fn = Camera.Shake,
      args = {
        StringToGuid("0x1"),
        "ShakeCameraMedium",
        ObjectState.GetLinkGuid(uOilrig, String.GetHash("hp_snap_oilrig_bld_helipadlargeA")),
        0.3
      }
    },
    {
      fn = ObjectState.StartEmitter,
      args = {
        uOilrig,
        String.GetHash("hp_fx_explosionJ"),
        uHugeExplosion
      },
      minTime = 0.2
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_bld_helipadlargeA"
      },
      minTime = 3,
      maxTime = 3.5
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_towerdrillpipesB"
      }
    },
    {
      fn = ObjectState.StartEmitter,
      args = {
        uOilrig,
        String.GetHash("hp_fx_explosionK"),
        uHugeExplosion
      },
      minTime = 0.8
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_fueltanklargeA"
      },
      minTime = 1
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_fueltanklargeA_B"
      }
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_fueltanklargeA_C"
      },
      minTime = 1
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_fueltanklargeA_A"
      }
    },
    {
      fn = ObjectState.StartEmitter,
      args = {
        uOilrig,
        String.GetHash("hp_fx_explosionL"),
        uLargeExplosion
      },
      minTime = 0.5
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_towerdrillpipesA"
      }
    },
    {
      fn = ObjectState.StartEmitter,
      args = {
        uOilrig,
        String.GetHash("hp_fx_explosionM"),
        uHugeExplosion
      },
      minTime = 0.5
    },
    {
      fn = ObjectState.StartEmitter,
      args = {
        uOilrig,
        String.GetHash("hp_fx_explosionN"),
        uHugeExplosion
      },
      minTime = 0.5
    },
    {
      fn = Camera.Shake,
      args = {
        StringToGuid("0x1"),
        "ShakeCameraMedium",
        ObjectState.GetLinkGuid(uOilrig, String.GetHash("hp_snap_oilrig_towerdrill")),
        0.8
      }
    },
    {
      fn = _DestroyLinkedGuid,
      args = {
        uOilrig,
        "hp_snap_oilrig_towerdrill"
      },
      minTime = 1.5,
      maxTime = 1.7
    },
    {
      fn = _DestroyBuildingB,
      args = {uOilrig},
      minTime = 1,
      maxTime = 1.5
    },
    {
      fn = _DestroyBuildingC,
      args = {uOilrig},
      minTime = 12,
      maxTime = 13
    },
    {
      fn = _FinishDestruction,
      args = {uOilrig}
    }
  }
  _ProcessNextEvent(uOilrig, dataTable, 1)
end

function _ProcessNextEvent(uiGuid, tEventTable, iIndex)
  local data = tEventTable[iIndex]
  if not data then
    return
  end
  data.fn(unpack(data.args))
  if not data.minTime then
    _ProcessNextEvent(uiGuid, tEventTable, iIndex + 1)
  else
    local newTime
    if data.maxTime then
      newTime = Math.randf(data.minTime, data.maxTime)
    else
      newTime = data.minTime
    end
    local e = Event.Create(Event.TimerRelative, {newTime}, _ProcessNextEvent, {
      uiGuid,
      tEventTable,
      iIndex + 1
    })
    local tEvents = _tOilrigEvents[uiGuid]
    if tEvents then
      table.insert(_tOilrigEvents[uiGuid], e)
    end
  end
end

function _DestroyLinkedGuid(uParent, sLinkName)
  local uLinkedObject = ObjectState.GetLinkGuid(uParent, String.GetHash(sLinkName))
  if not uLinkedObject then
    Debug.Printf("********** ERROR on ", sLinkName, uParent)
  else
    Object.Kill(uLinkedObject)
  end
end
