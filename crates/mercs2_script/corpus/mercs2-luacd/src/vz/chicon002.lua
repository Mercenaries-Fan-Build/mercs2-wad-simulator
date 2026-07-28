inherit("MrxTaskContract")
import("DangerousBuilding")
import("MrxVoSequence")
import("MrxSubtitle")
import("MrxSupportData")
import("MrxUtil")
import("MrxHQManager")

function LoadAssets(self, tSaveData)
  local tLayersToRemove = {
    "vz_state_staging_oildepot",
    "vz_state_staging_oilhq",
    "vz_state_mar_city_pristine"
  }
  local tLayersToAdd = {
    "vz_State_mar_city_ruined",
    "Vz_State_ChiCon002",
    "vz_state_Chicon002_Traffic",
    "vz_state_OC_Depot",
    "vz_state_OC_Depot_pristine"
  }
  if self:_GetFlag("DepotDestroyed_New") then
    Debug.Printf("~~~~~~~~~~~~~~~~~~* Depot Destroyed!")
    table.insert(tLayersToAdd, "vz_state_ChiCon002_Depot_Destroyed")
    table.insert(tLayersToRemove, "vz_state_ChiCon002_Depot_Pristine")
    table.insert(tLayersToRemove, "vz_state_ChiCon002_Depot_Hostiles")
  else
    Debug.Printf("~~~~~~~~~~~~~~~~~~* Depot Not Destroyed!")
    table.insert(tLayersToAdd, "vz_state_ChiCon002_Depot_Pristine")
    table.insert(tLayersToAdd, "vz_state_ChiCon002_Depot_Hostiles")
  end
  if self:_GetFlag("HQDestroyed_New") then
    Debug.Printf("~~~~~~~~~~~~~~~~~~* HQ Destroyed!")
    table.insert(tLayersToAdd, "vz_state_ChiCon002_HQ_Destroyed")
    table.insert(tLayersToRemove, "vz_state_ChiCon002_HQ_Pristine")
    table.insert(tLayersToRemove, "vz_state_ChiCon002_HQ_Hostiles")
  else
    Debug.Printf("~~~~~~~~~~~~~~~~~~* HQ Not Destroyed!")
    table.insert(tLayersToAdd, "vz_state_ChiCon002_HQ_Pristine")
    table.insert(tLayersToAdd, "vz_state_ChiCon002_HQ_Hostiles")
  end
  if self:_GetFlag("BridgeDestroyed_New") then
    Debug.Printf("~~~~~~~~~~~~~~~~~~* Bridge Destroyed!")
    table.insert(tLayersToAdd, "vz_state_ChiCon002_Bridge_Destroyed")
    table.insert(tLayersToRemove, "vz_state_ChiCon002_Bridge_Pristine")
    table.insert(tLayersToRemove, "vz_state_ChiCon002_Bridge_Hostiles")
  else
    Debug.Printf("~~~~~~~~~~~~~~~~~~* Bridge Not Destroyed!")
    table.insert(tLayersToAdd, "vz_state_ChiCon002_Bridge_Pristine")
    table.insert(tLayersToAdd, "vz_state_ChiCon002_Bridge_Hostiles")
  end
  MrxLayerManager.Remove(tLayersToRemove, function()
    MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  end)
end

function Activated(self)
  MrxTaskContract.Activated(self)
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_Maracaibo"), "warzonemar")
  MrxHqManager.SetHqRespawn("OilHq", false)
  if not self:_GetFlag("HQDestroyed_New") then
    _HQHealthBar(self)
  end
  if self:_GetFlag("DepotDestroyed_New") or self:_GetFlag("HQDestroyed_New") or self:_GetFlag("BridgeDestroyed_New") then
    tInitialVOTable = {}
  else
    tInitialVOTable = {
      "Fiona-In-Mission-Contract-Chi02-15",
      0.5,
      "Fiona-Banter-Contract-Chi02-01",
      0.5,
      {
        mattias = "mattias-Banter-Contract-Chi02-02",
        jennifer = "jennifer-Banter-Contract-Chi02-03",
        chris = "chris-Banter-Contract-Chi02-04"
      }
    }
  end
  if not self:_GetFlag("HQDestroyed_New") then
    self:CreateChild({
      sName = "Destroy OC Base",
      sModuleName = "MrxTaskObjectiveDestroy",
      vTgtInclude = Pg.GetGuidByName("_ocoutpost_bld_hq 0x000d3f3c"),
      sDspShortDesc = "[ChiCon002.Objectives.001]",
      tOnComplete = {
        {
          _HQDestroyedVO,
          {self}
        },
        {
          _CheckObjectiveCompletion,
          {self}
        }
      },
      tOnCancel = {
        {
          self.Cancel,
          {self}
        }
      }
    })
  end
  if not self:_GetFlag("DepotDestroyed_New") then
    self:CreateChild({
      sName = "Destroy OC Depot",
      sModuleName = "MrxTaskObjectiveDestroy",
      vTgtInclude = {
        "_industrial_bld_hangar01 0x000f8044",
        "_port_crane01 0x000f80bb",
        "_merida_bld_oilwellland 0x0010a85b",
        "_merida_bld_oilwellland 0x0010a85c",
        "_industrial_bld_warehousesmall01 0x000f5fe5",
        "_industrial_bld_warehousesmall01 0x000f807d",
        "_industrial_bld_warehousesmall02 0x000fec29",
        "_industrial_bld_warehousesmall01 0x000fec2a",
        "_industrial_bld_warehousesmall01 0x000f5fe4",
        "_industrial_bld_warehousesmall01 0x000f5fe3",
        "_industrial_bld_warehousesmall02 0x000d6dcb",
        "_industrial_bld_warehousesmall02 0x000eb0fc",
        "_industrial_bld_warehousesmall01 0x000f80df",
        "_industrial_bld_warehousesmall01 0x000f80e0",
        "_industrial_bld_warehousesmall02 0x000d6cdf"
      },
      sDspShortDesc = "[ChiCon002.Objectives.002]",
      fOnComplete = function()
        Debug.Printf("~~~~~~* Got this far")
        if uDepotMissedEvent then
          Event.Delete(uDepotMissedEvent)
        end
        self:_DepotDestroyedVO(self)
        self:_CheckObjectiveCompletion(self)
      end,
      fOnPartComplete = function(uDestroyedGuid)
        Debug.Printf("~~~~~~~~* Just destroyed: ", uDestroyedGuid)
        self:_DepotMissedVO()
      end,
      fOnCancel = function()
        self:Cancel()
      end
    })
  end
  if not self:_GetFlag("BridgeDestroyed_New") then
    self:CreateChild({
      sName = "Destroy Bridge",
      sModuleName = "MrxTaskObjectiveDestroy",
      vTgtInclude = {
        "_maracaibo_bridge_segmenta 0x0008b073",
        "_maracaibo_bridge_segmenta 0x00091fcc",
        "_maracaibo_bridge_segmenta 0x0008ab9b"
      },
      sDspShortDesc = "[ChiCon002.Objectives.003]",
      fOnComplete = function()
        Debug.Printf("~~~~~~* Got this far")
        if uBridgetMissedEvent then
          Event.Delete(uBridgeMissedEvent)
        end
        self:_BridgeDestroyedVO(self)
        self:_CheckObjectiveCompletion(self)
      end,
      fOnPartComplete = function(uDestroyedGuid)
        Debug.Printf("~~~~~~~~* Just destroyed: ", uDestroyedGuid)
        self:_BridgeMissedVO()
      end,
      fOnCancel = function()
        self.Cancel(self)
      end,
      vVoSeqOnAdd = tInitialVOTable
    })
  end
  _BridgeSpottedBoundary(self)
  _HQSpottedBoundary(self)
  _DepotSpottedBoundary(self)
end

function _SetupVehiclePatrol(self, sActor, sTarget, sPriority)
  self:_CreateEvent(Event.ObjectHibernation, {
    Vehicle.GetDriver(Pg.GetGuidByName(sActor)),
    "awake"
  }, _StartPatrol, {
    self,
    Vehicle.GetDriver(Pg.GetGuidByName(sActor)),
    Pg.GetGuidByName(sTarget),
    sPriority
  })
end

function _StartPatrol(self, uActor, uTarget, sPriority)
  tGoalParams = {
    AIGuid = uActor,
    Goal = "PathMove",
    Target = uTarget,
    Priority = "medPri"
  }
  self:_CreateEvent(Event.TimerRelative, {1}, Ai.Goal, {tGoalParams})
end

function _CheckObjectiveCompletion(self)
  if self:_GetFlag("DepotDestroyed_New") and self:_GetFlag("HQDestroyed_New") and self:_GetFlag("BridgeDestroyed_New") then
    Debug.Printf("~~~~~~~~~~~~~~~~~~* MISSION IS ENDING!!!")
    local tSequence = {
      "Fiona-In-Mission-Contract-Chi02-26",
      {
        self.Complete,
        {self}
      }
    }
    MrxVoSequence.Start(tSequence)
  end
end

function _HQHealthBar(self)
  local uOCHQguid = Pg.GetGuidByName("_ocoutpost_bld_hq 0x000d3f3c")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    GetGuidByName("LR_OC_HQ_Traffic_ChiCon002 0x000f3246"),
    "enter"
  }, function()
    MrxUtil.DisplayHealthBar(self, uOCHQguid, 0, true, 0)
    self:_CreateEvent(Event.Boundary, {
      Player.GetAnyCharacter(),
      GetGuidByName("LR_ChiCon2_HeloAttackHQ"),
      "exit"
    }, function()
      MrxUtil.StopHealthBar(uOCHQguid)
      _HQHealthBar(self)
    end)
  end)
end

function _BridgeSpottedBoundary(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetPrimaryCharacter(),
    GetGuidByName("LR_Gurcon001_BridgeView"),
    "enter"
  }, _BridgeSpottedVO, {self})
end

function _BridgeSpottedVO(self)
  if not self:_GetFlag("BridgeDestroyed_New") then
    if Object.IsVisible(Pg.GetGuidByName("_maracaibo_bridge_segmenta 0x00091fcc")) or Object.IsVisible(Pg.GetGuidByName("_maracaibo_bridge_segmentb 0x0008b074")) or Object.IsVisible(Pg.GetGuidByName("_maracaibo_bridge_segmentb 0x0008ab9c")) then
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Chi02-27"
      })
      Sound.SetActionLevelsMusic(15, 0, 0, 0)
    else
      self:_CreateEvent(Event.TimerRelative, {1}, _BridgeSpottedVO, {self})
    end
  end
end

function _BridgeMissedVO(self)
  if uBridgeMissedEvent then
    Event.Delete(uBridgeMissedEvent)
  end
  local tVo = {
    "Fiona-In-Mission-Contract-Chi02-32",
    "Fiona-In-Mission-Contract-Chi02-37",
    "Fiona-In-Mission-Contract-Chi02-38",
    "Fiona-In-Mission-Contract-Chi02-39"
  }
  local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
  uBridgeMissedEvent = self:_CreateEvent(Event.TimerRelative, {45}, MrxVoSequence.Start, {sSelectedVo})
end

function _BridgeDestroyedVO(self)
  Pg.EnableIntersection(false, StringToGuid("_maracaibo_bridge_segmentb 0x0008ab9c"))
  Pg.EnableIntersection(false, StringToGuid("_maracaibo_bridge_segmentb 0x0008b074"))
  if uBridgeMissedEvent then
    Event.Delete(uBridgeMissedEvent)
  end
  self:_SetFlag("BridgeDestroyed_New")
  if self:_GetFlag("DepotDestroyed_New") and self:_GetFlag("HQDestroyed_New") then
  elseif self:_GetFlag("DepotDestroyed_New") then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi02-19"
    })
  elseif self:_GetFlag("HQDestroyed_New") then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi02-18"
    })
  else
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi02-17"
    })
  end
  _Checkpoint({
    "CP_Bridge_P1",
    "CP_Bridge_P2"
  })
end

function _HQSpottedBoundary(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetPrimaryCharacter(),
    GetGuidByName("LR_ChiCon2_HeloAttackHQ"),
    "enter"
  }, _HQSpottedVO, {self})
end

function _HQSpottedVO(self)
  if not self:_GetFlag("HQDestroyed_New") then
    if Object.IsVisible(Pg.GetGuidByName("_ocoutpost_bld_hq 0x000d3f3c")) then
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Chi02-28"
      })
      Sound.SetActionLevelsMusic(15, 0, 0, 0)
    else
      self:_CreateEvent(Event.TimerRelative, {1}, _HQSpottedVO, {self})
    end
  end
end

function _HQDestroyedVO(self)
  local uOCHQguid = Pg.GetGuidByName("_ocoutpost_bld_hq 0x000d3f3c")
  MrxUtil.StopHealthBar(uOCHQguid)
  self:_SetFlag("HQDestroyed_New")
  if self:_GetFlag("DepotDestroyed_New") and self:_GetFlag("BridgeDestroyed_New") then
  elseif self:_GetFlag("DepotDestroyed_New") then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi02-25"
    })
  elseif self:_GetFlag("BridgeDestroyed_New") then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi02-24"
    })
  else
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi02-23"
    })
  end
  _Checkpoint({"CP_HQ_P1", "CP_HQ_P2"})
end

function _DepotSpottedBoundary(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetPrimaryCharacter(),
    GetGuidByName("LR_Gurcon001_DepotView"),
    "enter"
  }, _DepotSpottedVO, {self})
end

function _DepotSpottedVO(self)
  if not self:_GetFlag("DepotDestroyed_New") then
    if Object.IsVisible(Pg.GetGuidByName("_port_crane01 0x000f80bb")) or Object.IsVisible(Pg.GetGuidByName("_ocoutpost_wallgate 0x000f9a63")) or Object.IsVisible(Pg.GetGuidByName("_merida_bld_oilwellland 0x0010a85c")) then
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Chi02-29"
      })
      Sound.SetActionLevelsMusic(15, 0, 0, 0)
    else
      self:_CreateEvent(Event.TimerRelative, {1}, _DepotSpottedVO, {self})
    end
  end
end

function _DepotMissedVO(self)
  if uDepotMissedEvent then
    Event.Delete(uDepotMissedEvent)
  end
  local tVo = {
    "Fiona-In-Mission-Contract-Chi02-30",
    "Fiona-In-Mission-Contract-Chi02-36",
    "Fiona-In-Mission-Contract-Chi02-41"
  }
  local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
  uDepotMissedEvent = self:_CreateEvent(Event.TimerRelative, {45}, MrxVoSequence.Start, {sSelectedVo})
end

function _DepotDestroyedVO(self)
  if uDepotMissedEvent then
    Event.Delete(uDepotMissedEvent)
  end
  self:_SetFlag("DepotDestroyed_New")
  if self:_GetFlag("HQDestroyed_New") and self:_GetFlag("BridgeDestroyed_New") then
  elseif self:_GetFlag("HQDestroyed_New") then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi02-25"
    })
  elseif self:_GetFlag("BridgeDestroyed_New") then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi02-21"
    })
  else
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi02-20"
    })
  end
  _Checkpoint({
    "CP_Depot_P1",
    "CP_Depot_P2"
  })
end

function Cleanup(self)
  local uOCHQguid = Pg.GetGuidByName("_ocoutpost_bld_hq 0x000d3f3c")
  MrxUtil.StopHealthBar(uOCHQguid)
  MrxLayerManager.MarkForRemoval("vz_state_Chicon002_Traffic")
  MrxLayerManager.MarkForRemoval("Vz_State_ChiCon002")
  MrxLayerManager.MarkForRemoval("vz_state_ChiCon002_Depot_Hostiles")
  MrxLayerManager.MarkForRemoval("vz_state_ChiCon002_HQ_Hostiles")
  MrxLayerManager.MarkForRemoval("vz_state_ChiCon002_Bridge_Hostiles")
  MrxTaskContract.Cleanup(self)
end
