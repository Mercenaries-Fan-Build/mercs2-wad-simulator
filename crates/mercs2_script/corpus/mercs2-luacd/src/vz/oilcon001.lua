inherit("MrxTaskContract")
import("MrxSubtitle")
import("MrxUtil")
import("MrxApcDrop")
import("MrxVoSequence")
import("MrxMissionBoundary")
import("DangerousBuilding")
import("MrxSupportData")
import("MrxCopterDrop")
local tBurnObjConfigs = {}
tBurnObjConfigs[1] = {
  name = "warehouse2",
  sDesc = "[OilCon001.Objectives.warehouse2]",
  office = "refinery_office02",
  dropoffPoint = "oc001.obj2.loc.dropoff",
  returnPoint = "oc001.obj2.loc.return",
  hidePoint = "oc001.obj2.loc.hide",
  boxVal = 10,
  goalVal = 340,
  tVO = {
    "OilExec-In-Mission-Contract-Oil01-57",
    "OilExec-In-Mission-Contract-Oil01-58",
    "OilExec-In-Mission-Contract-Oil01-59"
  }
}
tBurnObjConfigs[2] = {
  name = "warehouse1",
  sDesc = "[OilCon001.Objectives.warehouse1]",
  office = "refinery_office03",
  dropoffPoint = "oc001.obj3.loc.dropoff",
  returnPoint = "oc001.obj3.loc.return",
  hidePoint = "oc001.obj3.loc.hide",
  boxVal = 10,
  goalVal = 200,
  tVO = {
    "OilExec-In-Mission-Contract-Oil01-62",
    "OilExec-In-Mission-Contract-Oil01-63",
    "OilExec-In-Mission-Contract-Oil01-64"
  }
}

function LoadAssets(self, tSaveData)
  local tLayersToRemove = {
    "vz_state_mar_industrial_act1"
  }
  local tLayersToAdd = {
    "vz_state_mar_industrial_pristine",
    "Vz_State_OilCon001"
  }
  if self:_GetFlag("StartSite2") == nil then
    table.insert(tLayersToAdd, "Vz_State_OilCon001_part1")
  else
    table.insert(tLayersToRemove, "Vz_State_OilCon001_part1")
  end
  DangerousBuilding.SetRarity("all", "never")
  MrxLayerManager.Remove(tLayersToRemove, function()
    MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  end)
end

function Activated(self)
  MrxTaskContract.Activated(self)
  
  function _MyCancel()
    self:Cancel()
  end
  
  local guid = Pg.GetGuidByName("oc001_exec")
  if self:_GetFlag("StartSite2") then
    _DestroyGate("mar_industrial_gate_north")
    _DestroyGate("_ocoutpost_wallgate 0x10001d5d")
    _DestroyGate("_ocoutpost_wallgate 0x000dac4e")
    MrxFactionManager.DisableReporting(true)
    self.exec = Executive:Create(self, guid, _MyCancel, Pg.GetGuidByName(tBurnObjConfigs[2].returnPoint))
    self:Obj_Site2_Goto()
  else
    self.exec = Executive:Create(self, guid, _MyCancel)
    self:FirstWarehouseDestroyedSetup()
    self:BuildingDestroyedSetup("refinery_office03")
    self:Obj_GotoRefinery()
    _SetupConvoy1()
  end
  self:BuildingDestroyedSetup("refinery_office02")
  self:SoundRegion_Outside()
end

function Cancel(self)
  if self.exec and not Object.IsAlive(self.exec.guid) then
    if not self.bCancelVoPlayed then
      self:_SetCancelMessage("[OilCon001.Terms.Cancel01]")
      local tExecVOs = {
        "OilExec-In-Mission-Contract-Oil01-49",
        "OilExec-In-Mission-Contract-Oil01-49",
        "OilExec-In-Mission-Contract-Oil01-50",
        "OilExec-In-Mission-Contract-Oil01-50",
        "OilExec-In-Mission-Contract-Oil01-51"
      }
      local tVO = {
        MrxUtil.GetRandomTableElement(tExecVOs),
        "Fiona-In-Mission-Contract-Oil01-16",
        {
          MrxTaskContract.Cancel,
          {self}
        }
      }
      MrxVoSequence.Start(tVO)
      self.bCancelVoPlayed = true
    end
  else
    MrxTaskContract.Cancel(self)
  end
end

function Cleanup(self)
  if self.curAttack then
    self.curAttack:Cleanup()
    self.curAttack = nil
  end
  if self.heliAttack then
    self.heliAttack:Cleanup()
    self.heliAttack = nil
  end
  if self.exec then
    self.exec:Cleanup()
    Object.Remove(self.exec.guid)
    self.exec = nil
  end
  if self.missionBoundary then
    self.missionBoundary:Cancel()
    self.missionBoundary = nil
  end
  Ai.RemoveExclusionZone()
  DangerousBuilding.SetRarity("all", "default")
  MrxFactionManager.DisableReporting(false)
  local tLayers = {
    "Vz_State_OilCon001",
    "Vz_State_OilCon001_part1"
  }
  for _, sLayer in pairs(tLayers) do
    MrxLayerManager.MarkForRemoval(sLayer)
  end
  MrxSupportData.RemoveFreebie("OilCon001_Crate")
  MrxTaskContract.Cleanup(self)
end

function Obj_GotoRefinery(self)
  self:CreateChild({
    sName = "oc001 go to",
    sModuleName = "MrxTaskObjectiveDeliver",
    vDestLoc = "refinery_doc_warehouse01",
    fDist = 120,
    bXZOnly = true,
    vTgtInclude = Player.GetAnyCharacter(),
    bStop = false,
    nQuota = 1,
    sDspShortDesc = "[OilCon001.Objectives.001]",
    fOnComplete = function()
      self:Obj_RescueExec()
    end,
    fOnCancel = _MyCancel,
    vVoSeqOnAdd = {
      "Fiona-Banter-Contract-Oil01-01",
      {
        mattias = "Mattias-Banter-Contract-Oil01-02",
        jennifer = "Jennifer-Banter-Contract-Oil01-03",
        chris = "Chris-Banter-Contract-Oil01-04"
      }
    }
  })
end

function Obj_RescueExec(self)
  local tVZsquad = Ai.Squad({
    SquadGuid = Pg.GetGuidByName("oc001.obj1.vzsquad1"),
    Action = "GetUnits"
  })
  local bAnySquadMemberAlive = false
  for i, uSoldier in pairs(tVZsquad) do
    if Object.IsAlive(uSoldier) then
      bAnySquadMemberAlive = true
      break
    end
  end
  if bAnySquadMemberAlive then
    self.curObj = self:CreateChild({
      sName = "oc001 rescue executive",
      sModuleName = "MrxTaskObjectiveDestroy",
      vTgtInclude = tVZsquad,
      bDspBlp = true,
      sDspShortDesc = "[OilCon001.Objectives.002]",
      tOnComplete = {
        {
          Obj_TalkToExec,
          {self}
        }
      },
      fOnCancel = _MyCancel,
      vVoSeqOnAdd = {
        "Fiona-In-Mission-Contract-Oil01-01"
      }
    })
  else
    self:Obj_TalkToExec()
  end
  self:CreateMissionBoundary({
    sPoint = "refinery_doc_warehouse01",
    tExitVOs = {
      "Fiona-In-Mission-Contract-Oil01-135",
      "Fiona-In-Mission-Contract-Oil01-18"
    },
    tWarnVOs = {
      "Fiona-In-Mission-Contract-Oil01-136"
    },
    fCallback = function()
      if Object.IsHibernated(self.exec.guid) then
        _MyCancel()
      else
        Object.Kill(self.exec.guid)
      end
    end
  })
  MrxFactionManager.DisableReporting(true)
end

function Obj_TalkToExec(self)
  local executive = self.exec.guid
  executive = executive or Pg.GetGuidByName("oc001_exec")
  self.curObj = nil
  self:CreateChild({
    sName = "talk to exec",
    sModuleName = "MrxTaskObjectiveAction",
    sActionLabel = "[ContextAction.Talk]",
    vTgtInclude = executive,
    bDsp = true,
    bDspMsg = true,
    sDspShortDesc = "[OilCon001.Objectives.003]",
    tOnPartComplete = {
      {
        ExecutiveIntroConversation,
        {self}
      }
    },
    fOnCancel = _MyCancel,
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Oil01-134"
    }
  })
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("oc001.rgn.warehouse01"),
    "enter",
    false
  }, _OCSavedBanter, {self})
end

function ObjDeliverExec(self, tConfig, fMyOnComplete, tInitVO, uPlayer)
  Ai.RemoveGoal({
    AIGuid = self.exec.guid,
    Handle = 0
  })
  self:CreateChild({
    sName = "Deliver exec to " .. tConfig.name,
    sModuleName = "MrxTaskObjectiveDeliver",
    vDestLoc = tConfig.dropoffPoint,
    fDist = tConfig.fDist or 3.5,
    bXZOnly = false,
    bStop = true,
    uStartAttachedToPlayer = uPlayer,
    vTgtInclude = self.exec.guid,
    sDspShortDesc = tConfig.sDesc,
    tOnComplete = {
      {
        fMyOnComplete,
        {self, tConfig}
      }
    },
    fOnCancel = _MyCancel,
    fAttachCallback = tConfig.fAttachCallback,
    tAttachCallbackData = {self},
    tStartFollowVO = {
      "OilExec-In-Mission-Contract-Oil01-28",
      "OilExec-In-Mission-Contract-Oil01-29",
      "OilExec-In-Mission-Contract-Oil01-30",
      "OilExec-In-Mission-Contract-Oil01-31"
    },
    tStopFollowVO = {
      "OilExec-In-Mission-Contract-Oil01-32",
      "OilExec-In-Mission-Contract-Oil01-33",
      "OilExec-In-Mission-Contract-Oil01-34"
    },
    tLostVO = {
      "OilExec-In-Mission-Contract-Oil01-38",
      "OilExec-In-Mission-Contract-Oil01-37",
      "OilExec-In-Mission-Contract-Oil01-39",
      "OilExec-In-Mission-Contract-Oil01-35",
      "OilExec-In-Mission-Contract-Oil01-36"
    },
    tFoundVO = {
      "OilExec-In-Mission-Contract-Oil01-43",
      "OilExec-In-Mission-Contract-Oil01-42",
      "OilExec-In-Mission-Contract-Oil01-40",
      "OilExec-In-Mission-Contract-Oil01-41"
    },
    tHostileVO = {
      "OilExec-In-Mission-Contract-Oil01-45",
      "OilExec-In-Mission-Contract-Oil01-48",
      "OilExec-In-Mission-Contract-Oil01-49",
      "OilExec-In-Mission-Contract-Oil01-47",
      "OilExec-In-Mission-Contract-Oil01-45",
      "OilExec-In-Mission-Contract-Oil01-48",
      "OilExec-In-Mission-Contract-Oil01-49",
      "OilExec-In-Mission-Contract-Oil01-46"
    },
    vVoSeqOnAdd = tInitVO
  })
end

function ExecutiveIntroConversation(self, uPlayer)
  self.exec:Stand(uPlayer)
  Ai.Goal({
    AIGuid = self.exec.guid,
    Goal = "Idle",
    Priority = "HiPri"
  })
  self:Obj_Site3_Goto(uPlayer)
  self:_RabbitSetup()
end

function Obj_Site2_Goto(self)
  self:_SetFlag("StartSite2")
  _Checkpoint({
    "oc001.loc.restart.p1",
    "oc001.loc.restart.p2"
  })
  local tVO = {
    {
      "OilExec-In-Mission-Contract-Oil01-60",
      self.exec.guid
    }
  }
  self:ObjDeliverExec(tBurnObjConfigs[1], self.Obj_Site2_Defend, tVO)
  _DestroyGate("mar_industrial_gate_north")
  self:CreateMissionBoundaryDelayed({
    sPoint = tBurnObjConfigs[1].dropoffPoint,
    tExitVOs = {
      "Fiona-In-Mission-Contract-Oil01-137",
      "Fiona-In-Mission-Contract-Oil01-138"
    },
    tWarnVOs = {
      "Fiona-In-Mission-Contract-Oil01-139"
    },
    fCallback = DocumentsCaptured
  })
end

function Obj_Site2_Defend(self, tConfig)
  local tAttackData = {
    office = "refinery_office02",
    atkPaths = {
      {
        "oc001.obj2.pth.atk1",
        "oc001.obj2.pth.atk2"
      },
      {
        "oc001.obj2.pth.atk3",
        "oc001.obj2.pth.atk4"
      },
      {
        "oc001.obj2.pth.atk5",
        "oc001.obj2.pth.atk6"
      }
    },
    atkPoints = {
      "oc001.obj2.loc.atk1",
      "oc001.obj2.loc.atk2",
      "oc001.obj2.loc.atk3"
    },
    defPoints = {
      "oc001.obj2.loc",
      "oc001.obj2.loc.atk4",
      "oc001.obj2.loc.atk5"
    },
    atkSpawnDist = {
      185,
      130,
      150
    },
    atkRanges = {
      90,
      90,
      70
    },
    waveVal = 10,
    tCmdList = {
      {
        "apc",
        "apc",
        1,
        12
      },
      {
        "setpath",
        {1},
        {
          {
            "OilExec-In-Mission-Contract-Oil01-61",
            self.exec.guid
          },
          1.5,
          {
            "Fiona-In-Mission-Contract-Oil01-94"
          }
        }
      },
      {
        "delay",
        4,
        1
      },
      {"wave"},
      {"wave"},
      {"wave"},
      {"wave"},
      {"wave"},
      {
        "setpath",
        {2, 3},
        {
          "Fiona-In-Mission-Contract-Oil01-97"
        }
      },
      {
        "call",
        _PerformBanter,
        {
          self,
          "oc001.obj2.loc.atk4",
          "OCMerc-In-Mission-Contract-Oil01-79",
          nil
        }
      },
      {"wave"},
      {"wave"},
      {
        "setpath",
        {
          1,
          2,
          3
        },
        {
          "Fiona-In-Mission-Contract-Oil01-100"
        }
      },
      {
        "delay",
        5,
        2
      },
      {"wave"},
      {
        "call",
        TowerDefenseNag,
        {self}
      },
      {"delay", 2},
      {
        "delay",
        45,
        3,
        {
          "Fiona-In-Mission-Contract-Oil01-143"
        }
      },
      {
        "call",
        TowerDefenseFreebie,
        {self}
      },
      {"delay", 14},
      {
        "call",
        StartHeliAttack,
        {self}
      }
    },
    tAPCInfo = {
      {
        inPath = "oc001.obj2.pth.apc_in1",
        defensePoint = "oc001.obj2.loc",
        squadName = "oc001.sq2-1"
      },
      {
        inPath = "oc001.obj2.pth.apc_in2",
        outPath = "oc001.obj2.pth.apc_out2",
        defensePoint = "oc001.obj2.loc",
        squadName = "oc001.sq2-1"
      }
    }
  }
  self.curAttack = AttackWaves:Create(self, tAttackData)
  
  function tConfig.fOnComplete()
    self:Obj_Site2_Complete()
  end
  
  self.exec:Start(tConfig)
  self.curAttack:Start()
  self:SetupHeliTimeout()
end

function Obj_Site2_Complete(self)
  self.curAttack:Cleanup()
  self.curAttack = nil
  if self.heliAttack then
    self.heliAttack:Cleanup()
    self.heliAttack = nil
  end
  self:BuildingDestroyedCleanup("refinery_office02")
  self:ObjReturnExecutive()
  MrxMusic.StopSpecialMusic("action")
end

function DocumentsCaptured(self)
  self:_SetCancelMessage("[OilCon001.Terms.Cancel03]")
  self:Cancel()
end

function StartHeliAttack(self)
  Event.Delete(self._tEvents.eHeliTimeout)
  self._tEvents.eHeliTimeout = nil
  self.heliAttack = HeliAttack:Create(self, {
    tInPaths = {
      "oc001.pth.heli.in1",
      "oc001.pth.heli.in2",
      "oc001.pth.heli.in3",
      "oc001.pth.heli.in4"
    },
    tLoopPaths = {
      "oc001.pth.heli.loop1",
      "oc001.pth.heli.loop2",
      "oc001.pth.heli.loop3",
      "oc001.pth.heli.loop4"
    }
  })
  self.heliAttack:Start()
end

function SetupHeliTimeout(self)
  self._tEvents.eHeliTimeout = Event.Create(Event.TimerRelative, {
    tBurnObjConfigs[1].goalVal * 0.6 * 12 / tBurnObjConfigs[1].boxVal
  }, HeliTimeout, {self})
end

function HeliTimeout(self)
  self.curAttack.curCmd = 17
  if self.curAttack.attackObj then
    self.curAttack:BlipAttackers(false)
    self.curAttack:WaveStagnated()
  end
end

function TowerDefenseFreebie(self)
  Event.Delete(self._tEvents.eTowerDefenseNag)
  self._tEvents.eTowerDefenseNag = nil
  local uPoint = Pg.GetGuidByName("oc001.obj2.loc.supplyDrop")
  local x, y, z = Object.GetPosition(uPoint)
  local uHeli = MrxCopterDrop.Create("OC", "Supply Drop (AA)", x, y, z, true)
  self:_CreateEvent(Event.ObjectProximity, {
    uHeli,
    uPoint,
    "<",
    20,
    false,
    true
  }, TowerDefenseFreebieVO, {self})
  self:_PlayVo(0, "OCMerc-In-Mission-Contract-Oil01-118")
  if self.curAttack then
    self.curAttack:BlipAttackers(false)
  end
end

function TowerDefenseFreebieVO(self)
  MrxVoSequence.Start({
    "OCMerc-In-Mission-Contract-Oil01-119",
    0,
    {
      TowerDefenseAddFreebie,
      {self}
    },
    0.5,
    {
      mattias = "Mattias-In-Mission-Contract-Oil01-117",
      jennifer = "Jennifer-In-Mission-Contract-Oil01-116",
      chris = "Chris-In-Mission-Contract-Oil01-92"
    }
  })
end

function TowerDefenseAddFreebie(self)
  if Player.GetSecondaryCharacter() then
    MrxSupportData.AddFreebie("OilCon001_Crate", 1, Player.GetPrimaryPlayer())
    MrxSupportData.AddFreebie("OilCon001_Crate", 1, Player.GetSecondaryPlayer())
  else
    MrxSupportData.AddFreebie("OilCon001_Crate", 2, Player.GetPrimaryPlayer())
  end
end

function TowerDefenseNag(self)
  Event.Delete(self._tEvents.eHeliTimeout)
  self._tEvents.eHeliTimeout = nil
  self._tEvents.eTowerDefenseNag = Event.Create(Event.TimerRelative, {30}, self._PlayVo, {
    self,
    0,
    "Fiona-In-Mission-Contract-Oil01-102"
  })
end

function Obj_Site3_Goto(self, uPlayer)
  self:_StagingSetup()
  _DestroyGate("_ocoutpost_wallgate 0x000dac4e")
  local sIntroLine = "OilExec-In-Mission-Contract-Oil01-19"
  if Player.GetCurrentPlayers() > 1 then
    sIntroLine = "OilExec-In-Mission-Contract-Oil01-85"
  end
  local tVO = {
    nBaseDelay = 0,
    {
      Human.DoAction,
      {
        self.exec.guid,
        "SpeakGestureUB"
      }
    },
    {
      sIntroLine,
      self.exec.guid
    },
    {
      Human.DoAction,
      {
        self.exec.guid,
        "ExitAction"
      }
    },
    {
      mattias = "Mattias-In-Mission-Contract-Oil01-20",
      jennifer = "Jennifer-In-Mission-Contract-Oil01-21",
      chris = "Chris-In-Mission-Contract-Oil01-22"
    },
    {
      Human.DoAction,
      {
        self.exec.guid,
        "SpeakGestureUB"
      }
    },
    {
      "OilExec-In-Mission-Contract-Oil01-23",
      self.exec.guid
    },
    {
      Human.DoAction,
      {
        self.exec.guid,
        "ExitAction"
      }
    }
  }
  self:ObjDeliverExec(tBurnObjConfigs[2], self.Obj_Site3_Defend, tVO, uPlayer)
  local res = self:_CreateEvent(Event.ObjectProximity, {
    self.exec.guid,
    Pg.GetGuidByName("refinery_office03"),
    "<",
    50,
    false,
    false
  }, function()
    MrxVoSequence.Start({
      {
        mattias = "Mattias-In-Mission-Contract-Oil01-76",
        jennifer = "Jennifer-In-Mission-Contract-Oil01-77",
        chris = "Chris-In-Mission-Contract-Oil01-78"
      }
    })
  end)
  _MoveOCSquad("oc001.obj1.ocsquad1", "oc001.obj2.loc")
  self:CreateMissionBoundaryDelayed({
    sPoint = tBurnObjConfigs[2].dropoffPoint,
    tExitVOs = {
      "Fiona-In-Mission-Contract-Oil01-137",
      "Fiona-In-Mission-Contract-Oil01-138"
    },
    tWarnVOs = {
      "Fiona-In-Mission-Contract-Oil01-139"
    },
    fCallback = DocumentsCaptured
  })
end

function Obj_Site3_Defend(self, tConfig)
  function tConfig.fOnComplete()
    self.curAttack:Cleanup()
    
    self.curAttack = nil
    self:BuildingDestroyedCleanup("refinery_office03")
    self:Obj_Site2_Goto()
    MrxMusic.StopSpecialMusic()
  end
  
  self.exec:Start(tConfig)
  local fTime = Math.randf(150, 180)
  self:_CreateEvent(Event.TimerRelative, {fTime}, SpawnPipeRunners)
  local tAttackData = {
    office = "refinery_office03",
    atkPaths = {
      {
        "oc001.obj3.pth.atk01",
        "oc001.obj3.pth.atk02"
      },
      {
        "oc001.obj3.pth.atk21",
        "oc001.obj3.pth.atk22"
      }
    },
    atkPoints = {
      "oc001.obj3.loc.vz1",
      "oc001.obj3.loc.vz2"
    },
    defPoints = {
      "oc001.obj3.loc.atk1",
      "oc001.obj3.loc.atk2"
    },
    atkSpawnDist = {180, 150},
    atkRanges = {80, 70},
    waveVal = 5,
    tCmdList = {
      {
        "apc",
        "truck",
        1,
        10
      },
      {
        "setpath",
        {1}
      },
      {
        "delay",
        20,
        1,
        {
          "Fiona-In-Mission-Contract-Oil01-93"
        }
      },
      {"wave"},
      {"wave"},
      {"wave"},
      {
        "setpath",
        {2}
      },
      {
        "apc",
        "truck",
        2,
        5
      },
      {
        "call",
        _MoveOCSquad,
        {
          "oc001.obj1.ocsquad2",
          "oc001.obj3.loc.vz2"
        }
      },
      {
        "call",
        _MoveOCBoatSoldier,
        {false}
      },
      {
        "delay",
        60,
        2,
        {
          "Fiona-In-Mission-Contract-Oil01-95"
        }
      },
      {"wave"},
      {"wave"},
      {"wave"},
      {"wave"},
      {
        "setpath",
        {1}
      },
      {
        "apc",
        "truck",
        1,
        5
      },
      {
        "call",
        _MoveOCSquad,
        {
          "oc001.obj1.ocsquad2",
          "oc001.obj3.loc.vz1"
        }
      },
      {
        "call",
        _MoveOCBoatSoldier,
        {true}
      },
      {
        "delay",
        12,
        1,
        {
          "Fiona-In-Mission-Contract-Oil01-96"
        }
      },
      {"wave"},
      {"wave"},
      {"wave"},
      {"wave"}
    },
    tAPCInfo = {
      {
        inPath = "oc001.obj3.pth.apc_in1",
        outPath = "oc001.obj3.pth.apc_out1",
        defensePoint = "oc001.obj3.loc.vz1",
        squadName = "oc001.sq1-1"
      },
      {
        inPath = "oc001.obj3.pth.apc_in2",
        outPath = "oc001.obj3.pth.apc_out2",
        defensePoint = "oc001.obj3.loc.vz2",
        squadName = "oc001.sq1-2"
      }
    }
  }
  self.curAttack = AttackWaves:Create(self, tAttackData)
  MrxVoSequence.Start({
    {
      "OilExec-In-Mission-Contract-Oil01-56",
      self.exec.guid
    },
    {
      self.curAttack.Start,
      {
        self.curAttack
      }
    }
  })
end

function SpawnPipeRunners()
  local uPipe = Pg.GetGuidByName("_industrial_att_pipelargeshort 0x000eef58")
  if Object.IsAlive(uPipe) then
    local uPath = Pg.GetGuidByName("oc001.obj3.pth.pipe1")
    
    function SoldierRun(uSoldier, uPath, fSpeed)
      local h = Ai.Goal({
        AIGuid = uSoldier,
        Goal = "PathMove",
        Target = uPath,
        Haste = fSpeed,
        Mode = "OneWay",
        Priority = "HiPri"
      })
    end
    
    function SoldierSpawn(sStartPt, uPath, fSpeed)
      local uPoint = Pg.GetGuidByName(sStartPt)
      local x, y, z = Object.GetPosition(uPoint)
      local uGuy = Pg.Spawn("VZ Soldier", x, y, z, Object.GetYaw(uPoint), false, true)
      local h = Event.Create(Event.ObjectHibernation, {uGuy, "awake"}, SoldierRun, {
        uGuy,
        uPath,
        fSpeed
      })
    end
    
    SoldierSpawn("oc001.obj3.loc.pipespawn1", uPath, 0.5)
    SoldierSpawn("oc001.obj3.loc.pipespawn2", uPath, 0.45)
  end
end

function ObjReturnExecutive(self)
  local tDeliverConfig = {
    name = "UP HQ",
    fDist = 6,
    sDesc = "[OilCon001.Objectives.004]",
    dropoffPoint = "oc001.loc.finish",
    fAttachCallback = SetupReturnDriveBanter
  }
  self:ObjDeliverExec(tDeliverConfig, ObjReturnExecutiveComplete, {
    {
      "OilExec-In-Mission-Contract-Oil01-65",
      self.exec.guid
    }
  })
  MrxFactionManager.DisableReporting(false)
  self.missionBoundary:Cancel()
  self.missionBoundary = nil
  Event.Delete(self._tEvents.eMissionBoundary)
  self._tEvents.eMissionBoundary = nil
end

function SetupReturnDriveBanter(self, sMode, uGuid, bState)
  local function PlayReturnBanter()
    MrxVoSequence.Start({
      nBaseDelay = 0.4,
      
      {
        "OilExec-In-Mission-Contract-Oil01-126",
        self.exec.guid
      },
      {
        mattias = "Mattias-In-Mission-Contract-Oil01-104",
        jennifer = "Jennifer-In-Mission-Contract-Oil01-105",
        chris = "Chris-In-Mission-Contract-Oil01-106"
      },
      {
        "OilExec-In-Mission-Contract-Oil01-127",
        self.exec.guid
      },
      {
        mattias = "Mattias-In-Mission-Contract-Oil01-107",
        jennifer = "Jennifer-In-Mission-Contract-Oil01-108",
        chris = "Chris-In-Mission-Contract-Oil01-109"
      },
      {
        "OilExec-In-Mission-Contract-Oil01-130",
        self.exec.guid
      },
      {
        mattias = "Mattias-In-Mission-Contract-Oil01-110",
        jennifer = "Jennifer-In-Mission-Contract-Oil01-111",
        chris = "Chris-In-Mission-Contract-Oil01-112"
      },
      0,
      {
        "OilExec-In-Mission-Contract-Oil01-131",
        self.exec.guid
      },
      0,
      {
        "OilExec-In-Mission-Contract-Oil01-132",
        self.exec.guid
      },
      {
        mattias = "Mattias-In-Mission-Contract-Oil01-113",
        jennifer = "Jennifer-In-Mission-Contract-Oil01-114",
        chris = "Chris-In-Mission-Contract-Oil01-115"
      },
      0,
      {
        "OilExec-In-Mission-Contract-Oil01-133",
        self.exec.guid
      }
    })
  end
  
  if not self._tEvents.eReturnDriveBanter and bState then
    MrxVoSequence.Start({
      {
        mattias = "Mattias-In-Mission-Contract-Oil01-68",
        jennifer = "Jennifer-In-Mission-Contract-Oil01-69",
        chris = "Chris-In-Mission-Contract-Oil01-70"
      },
      0,
      {
        Human.DoAction,
        {
          self.exec.guid,
          "SpeakGestureUB"
        }
      },
      {
        "OilExec-In-Mission-Contract-Oil01-71",
        self.exec.guid
      },
      {
        Human.DoAction,
        {
          self.exec.guid,
          "ExitAction"
        }
      },
      0.5,
      {
        mattias = "Mattias-In-Mission-Contract-Oil01-86",
        jennifer = "Jennifer-In-Mission-Contract-Oil01-87",
        chris = "Chris-In-Mission-Contract-Oil01-74"
      }
    })
    self._tEvents.eReturnDriveBanter = Event.Create(Event.ObjectProximity, {
      self.exec.guid,
      Pg.GetGuidByName("oc001.loc.finish"),
      "<",
      1050,
      false,
      true
    }, PlayReturnBanter)
  end
end

function ObjReturnExecutiveComplete(self)
  local veh = Object.InSeat(self.exec.guid)
  if veh then
    Vehicle.Exit(veh, self.exec.guid)
  end
  
  local function MoveExecBackToHQ(self)
    local res = Ai.Goal({
      AIGuid = self.exec.guid,
      Goal = "MoveTo",
      Target = Pg.GetGuidByName("Starter_Oil0_Start1"),
      Haste = 0.65,
      Priority = "HiPri",
      Callback = MissionComplete,
      CallbackData = {self}
    })
  end
  
  MrxVoSequence.Start({
    {
      Human.DoAction,
      {
        self.exec.guid,
        "SpeakGestureUB"
      }
    },
    {
      "OilExec-In-Mission-Contract-Oil01-66",
      self.exec.guid
    },
    {
      Human.DoAction,
      {
        self.exec.guid,
        "ExitAction"
      }
    },
    {
      MoveExecBackToHQ,
      {self}
    }
  })
end

function MissionComplete(self)
  Object.FadeOut(self.exec.guid, 1, true)
  self:Complete()
end

vehInfo = {
  jeep = {
    template = "M151 .50Cal (VZ) (DriverGunner)",
    speed = 0.5,
    threat = 5
  },
  guntruck = {
    template = "M35 (Guntruck) (VZ) (Full)",
    speed = 0.45,
    threat = 10
  },
  truck = {
    template = "M35 (Cargo) (VZ) (Full RPG)",
    speed = 0.5,
    threat = 5
  },
  apc = {
    template = "M113 (VZ) (Full RPG)",
    speed = 1,
    threat = 12
  },
  tank = {
    template = "Scorpion90 (Driver)",
    speed = 0.9,
    threat = 0
  },
  heli = {
    template = "Alouette3 Attack (VZ) (Full)",
    speed = 1,
    threat = 0,
    loopspeed = 0.6
  }
}
AttackWaves = {}

function AttackWaves:Create(parent, t)
  if not t then
    Debug.Printf("AttackWaves:Create - no configuration supplied")
    return
  end
  setmetatable(t, self)
  self.__index = self
  t.parent = parent
  t.kMaxThreat = t.kMaxThreat or 200
  t.curCmd = 0
  t.curThreat = 0
  t.tVehicleEvents = {}
  parent.curWaveBonus = 0
  local uHumanFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(uHumanFilter, "human")
  t.eHumanThreat = Event.CreatePersistent(Event.ObjectDeath, {uHumanFilter}, t.HumanDied, {t})
  return t
end

function AttackWaves:Start()
  MrxMusic.PlaySpecialMusic("mu_fac_oc_threat_01")
  self:ProcessNextCmd()
end

function AttackWaves:Cleanup()
  Event.Delete(self.eHumanThreat)
  Event.Delete(self.eSpawnNextWave)
  Event.Delete(self.eDelayTimer)
  Event.Delete(self.eStagnateTimer)
  if self.oDelayProx then
    self.oDelayProx:Complete()
    self.oDelayProx = nil
  end
  for uVeh, tEvents in pairs(self.tVehicleEvents) do
    if tEvents.moveGoal then
      tEvents.moveGoal = nil
    end
    for i, e in pairs(tEvents) do
      Event.Delete(e)
    end
  end
  if self.attackObj then
    self.attackObj:Cancel()
    self.attackObj = nil
  end
  if self.curApc then
    self.curApc:Cleanup()
    self.curApc = nil
  end
end

function AttackWaves:UpdateThreat(iInc)
  self.curThreat = self.curThreat + iInc
  if self.curThreat > self.kMaxThreat then
    self.curThreat = self.kMaxThreat
  elseif self.curThreat < 0 then
    self.curThreat = 0
  end
end

function AttackWaves:UpdateDisplay()
  if gbDebug then
    Hud.ObjectiveTray:SetSlotToText({
      nSlot = 3,
      sText = "[yellow]Threat: " .. self.curThreat
    })
  end
end

function AttackWaves:BlipAttackers(bShow)
  if bShow and not self.bBlipAttackers and self.attackObj then
    self.attackObj:Configure({bDspBlp = true})
    self.bBlipAttackers = bShow
  end
end

function AttackWaves:ProcessNextCmd()
  self.curCmd = self.curCmd + 1
  local tCmd = self.tCmdList[self.curCmd]
  if not tCmd then
    return
  end
  if tCmd[1] == "wave" then
    self:SpawnNextWave()
  elseif tCmd[1] == "setpath" then
    self:SetPaths(tCmd[2], tCmd[3])
  elseif tCmd[1] == "delay" then
    self:DelayCmd(tCmd[2], tCmd[3], tCmd[4])
  elseif tCmd[1] == "apc" then
    self:SpawnApc(tCmd[2], tCmd[3], tCmd[4])
  elseif tCmd[1] == "call" then
    MrxUtil.CallWithOptionalArgs(tCmd[2], tCmd[3])
    self:ProcessNextCmd()
  else
    Debug.Printf("ProcessNextCmd: unknown cmd ", tostring(tCmd[1]))
  end
end

function AttackWaves:SetPaths(tPoints, tVO)
  if #tPoints == 1 then
    self.curPoints = tPoints[1]
    self.spawnFn = self.SpawnOneSquad
  else
    self.curPoints = tPoints
    self.spawnFn = self.SpawnManySquads
  end
  if tVO then
    table.insert(tVO, {
      self.ProcessNextCmd,
      {self}
    })
    MrxVoSequence.Start(tVO)
  else
    self:ProcessNextCmd()
  end
end

function AttackWaves:DelayCmd(fTime, iPointIdx, tVo)
  Event.Delete(self.eDelayTimer)
  if self.oDelayProx then
    self.oDelayProx:Complete()
  end
  self.eDelayTimer = Event.Create(Event.TimerRelative, {fTime}, self._DelayCmdComplete, {self})
  if iPointIdx then
    self.oDelayProx = self.parent:CreateChild({
      sName = "oc001 defend point",
      sModuleName = "MrxTaskObjectiveDeliver",
      vDestLoc = self.defPoints[iPointIdx],
      fDist = 12,
      bXZOnly = false,
      vTgtInclude = Player.GetAnyCharacter(),
      bStop = false,
      sDspShortDesc = "[OilCon001.Objectives.goto]",
      bDspMsg = false,
      vVoSeqOnAdd = tVo,
      fOnComplete = function()
        self:_DelayCmdComplete()
      end
    })
  end
end

function AttackWaves:_DelayCmdComplete()
  Event.Delete(self.eDelayTimer)
  if self.oDelayProx then
    self.oDelayProx:Cancel()
    self.oDelayProx = nil
  end
  self:ProcessNextCmd()
end

function AttackWaves:SpawnNextWave()
  local time = math.randf(0.8, 1.5)
  self.eSpawnNextWave = Event.Create(Event.TimerRelative, {time}, AttackWaves.SetupObjective, {self})
end

function AttackWaves:WaveCompleted()
  self.parent.curWaveBonus = self.parent.curWaveBonus + self.waveVal
  Event.Delete(self.eStagnateTimer)
  self.eStagnateTimer = nil
  Debug.Printf("----= WAVE COMPLETED")
end

function AttackWaves:WaveStagnated()
  self.eStagnateTimer = nil
  Debug.Printf("----= WAVE STAGNATED")
  for j, uVehicle in pairs(self.tAttackers) do
    local tEvents = self.tVehicleEvents[uVehicle]
    if tEvents then
      for i, e in pairs(tEvents) do
        Event.Delete(e)
      end
      self.tVehicleEvents[uVehicle] = nil
    end
  end
  self.attackObj:Complete()
end

function AttackWaves:SetupObjective()
  self.tAttackers = nil
  self.tAttackers = {}
  if not self.spawnFn then
    Debug.Printf("Error in SetupObjective: no spawnFn, must issue 'setpath' command first ")
    return
  end
  self.spawnFn(self, self.curPoints)
  if self.attackObj then
    self.attackObj:Cancel()
  end
  self.bBlipAttackers = false
  self.attackObj = self.parent:CreateChild({
    sName = "INTERNAL OBJ: wave death",
    sModuleName = "MrxTaskObjectiveDestroy",
    bDspMsg = false,
    bDspDescPda = false,
    bDspBlp = false,
    bDspBlpWld = false,
    bDspBlpPda = false,
    vTgtInclude = self.tAttackers,
    sDspShortDesc = "[OilCon001.Objectives.attackwaves]",
    fOnPartComplete = function(uVehicle)
      local tEvents = self.tVehicleEvents[uVehicle]
      if tEvents.moveGoal then
        tEvents.moveGoal = nil
      end
      for i, e in pairs(tEvents) do
        Event.Delete(e)
      end
      self.tVehicleEvents[uVehicle] = nil
      local nTotal = #self.tAttackers
      if 1 < nTotal and nTotal - self.attackObj:GetProgressCompleted() == 1 then
        if self.eStagnateTimer then
          Event.Delete(self.eStagnateTimer)
        end
        Debug.Printf("----= STARTING STAGNATION TIMER ", nTotal)
        self.eStagnateTimer = Event.Create(Event.TimerRelative, {18}, AttackWaves.WaveStagnated, {self})
      end
    end,
    fOnComplete = function()
      self:WaveCompleted()
      self:ProcessNextCmd()
    end
  })
end

function AttackWaves:SpawnManySquads(tIndices)
  for _, idx in ipairs(tIndices) do
    self:UpdateThreat(-0.1 * self.kMaxThreat)
    self:SpawnOneSquad(idx)
  end
end

function AttackWaves:SpawnOneSquad(index)
  local tPathSet = self.atkPaths[index]
  local fDist = self.atkSpawnDist[index]
  local fRange = self.atkRanges[index]
  local iBlipGuid = Pg.GetGuidByName(self.atkPoints[index])
  local curThreat = self.curThreat
  local i = math.randi(1, table.getn(tPathSet))
  if curThreat < 0.1 * self.kMaxThreat then
    self:SpawnVehicle(vehInfo.jeep, tPathSet[i], fDist, iBlipGuid, fRange)
  elseif curThreat < 0.25 * self.kMaxThreat then
    self:SpawnVehicle(vehInfo.jeep, tPathSet[i], fDist, iBlipGuid, fRange)
    i = math.randi(1, table.getn(tPathSet))
    self:SpawnVehicle(vehInfo.jeep, tPathSet[i], fDist - 10, iBlipGuid, fRange)
  elseif curThreat < 0.45 * self.kMaxThreat then
    self:SpawnVehicle(vehInfo.jeep, tPathSet[1], fDist, iBlipGuid, fRange)
    self:SpawnVehicle(vehInfo.jeep, tPathSet[2], fDist, iBlipGuid, fRange)
  elseif curThreat < 0.65 * self.kMaxThreat then
    self:SpawnVehicle(vehInfo.guntruck, tPathSet[i], fDist + 10, iBlipGuid, fRange)
  elseif curThreat < 0.8 * self.kMaxThreat then
    self:SpawnVehicle(vehInfo.jeep, tPathSet[i], fDist, iBlipGuid, fRange)
    i = math.randi(1, table.getn(tPathSet))
    self:SpawnVehicle(vehInfo.guntruck, tPathSet[i], fDist + 15, iBlipGuid, fRange)
  elseif curThreat < 0.9 * self.kMaxThreat then
    self:SpawnVehicle(vehInfo.jeep, tPathSet[1], fDist, iBlipGuid, fRange)
    self:SpawnVehicle(vehInfo.jeep, tPathSet[2], fDist, iBlipGuid, fRange)
    self:SpawnVehicle(vehInfo.jeep, tPathSet[i], fDist - 15, iBlipGuid, fRange)
  else
    self:SpawnVehicle(vehInfo.guntruck, tPathSet[i], fDist + 10, iBlipGuid, fRange)
    i = math.randi(1, table.getn(tPathSet))
    self:SpawnVehicle(vehInfo.guntruck, tPathSet[i], fDist + 20, iBlipGuid, fRange)
  end
end

function AttackWaves:SpawnVehicle(tVehInfo, sPath, fDist, uBackupPoint, fRange)
  local uPath = Pg.GetGuidByName(sPath)
  local res, x, y, z, yaw = MrxUtil.FindSpawnPointOutOfView(uPath, fDist)
  if not res then
    res, x, y, z, yaw = Pg.GetDistantSpawnPointOnPath(uPath, uBackupPoint, 0, fDist)
  end
  local uVehicle = Pg.Spawn(tVehInfo.template, x, y, z, yaw, false, true)
  local uDriver = Vehicle.GetDriver(uVehicle)
  local tEvents = {}
  tEvents.spawnDelay = Event.Create(Event.ObjectHibernation, {uVehicle, "awake"}, self.SetupSpawnedVehicle, {
    self,
    uVehicle,
    tVehInfo,
    uPath,
    uBackupPoint,
    fRange
  })
  self.tVehicleEvents[uVehicle] = tEvents
  table.insert(self.tAttackers, uVehicle)
end

function AttackWaves:SetupSpawnedVehicle(uVehicle, tVehInfo, uPath, uGoalPoint, fRange)
  local uDriver = Vehicle.GetDriver(uVehicle)
  local tEvents = self.tVehicleEvents[uVehicle]
  tEvents.driverDeath = Event.Create(Event.ObjectDeath, {uDriver}, self.VehicleDestroyed, {self, uVehicle})
  tEvents.driverJacked = Event.Create(Event.ObjectInSeat, {
    uDriver,
    uVehicle,
    "d",
    "x"
  }, self.VehicleDestroyed, {self, uVehicle})
  tEvents.moveGoal = Ai.Goal({
    AIGuid = uDriver,
    Goal = "PathMove",
    Target = uPath,
    Haste = tVehInfo.speed,
    Priority = "HiPri",
    Callback = self.VehicleAtDest,
    CallbackData = {self, uVehicle}
  })
  tEvents.eTimeout = Event.Create(Event.TimerRelative, {15}, self.VehicleTimedOut, {self, uVehicle})
  tEvents.eProx = Event.Create(Event.ObjectProximity, {
    uVehicle,
    uGoalPoint,
    "<",
    fRange,
    false,
    false
  }, self.VehicleAtGoalPoint, {
    self,
    uVehicle,
    tVehInfo.speed
  })
  tEvents.vehDeath = Event.Create(Event.ObjectDeath, {uVehicle}, self.UpdateThreat, {
    self,
    tVehInfo.threat
  })
end

function AttackWaves:VehicleAtDest(uVehicle, uDriver, iState)
  if 0 < iState then
    Ai.Goal({
      AIGuid = uDriver,
      Goal = "Attack",
      Target = Pg.GetGuidByName(self.office),
      Priority = "MedPri"
    })
  else
    Object.Kill(uVehicle)
  end
end

function AttackWaves:VehicleDestroyed(uVehicle, uDriver)
  if Object.GetHealth(uVehicle) > 0 then
    self.attackObj:_TargetDestroyed(uVehicle)
  end
end

function AttackWaves:HumanDied(uHuman)
  if Object.HasLabel(uHuman, "vz") then
    self:UpdateThreat(3)
  elseif Object.HasLabel(uHuman, "oc") then
    self:UpdateThreat(-6)
  end
end

function AttackWaves:VehicleAtGoalPoint(uVehicle, iCurSpeed)
  local tEvents = self.tVehicleEvents[uVehicle]
  Event.Delete(tEvents.eTimeout)
  tEvents.eProx = nil
  tEvents.eTimeout = nil
  local uDriver = Vehicle.GetDriver(uVehicle)
  Ai.SetHaste(uDriver, iCurSpeed * 0.5)
  self:BlipAttackers(true)
end

function AttackWaves:VehicleTimedOut(uVehicle)
  local tEvents = self.tVehicleEvents[uVehicle]
  Event.Delete(tEvents.eProx)
  tEvents.eProx = nil
  tEvents.eTimeout = nil
  Object.Kill(uVehicle)
end

HeliAttack = {}

function HeliAttack:Create(parent, t)
  if not t then
    Debug.Printf("AttackWaves:Create - no configuration supplied")
    return
  end
  setmetatable(t, self)
  self.__index = self
  t.parent = parent
  t.tVehicleEvents = {}
  t.tOrbiters = {}
  t.nCount = 0
  return t
end

function HeliAttack:Start()
  self:_Spawn(vehInfo.heli, 270, HeliAttack._StartStrafe)
  self:_Spawn(vehInfo.heli, 290, HeliAttack._StartOrbit)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Oil01-82"
  })
  MrxMusic.PlaySpecialMusic("mu_fac_oc_kickass_01")
end

function HeliAttack:Cleanup()
  for uVeh, tEvents in pairs(self.tVehicleEvents) do
    self:_CleanupOne(tEvents)
    self.tVehicleEvents[uVeh] = nil
  end
  Event.Delete(self.eStrafeTimer)
  self.eStrafeTimer = nil
end

function HeliAttack:_CleanupOne(tEvents)
  if tEvents.aiGoal then
    tEvents.aiGoal = nil
  end
  for i, e in pairs(tEvents) do
    Event.Delete(e)
  end
end

function HeliAttack:_Spawn(tVehInfo, fDist, fCallback)
  local i = Math.randi(1, #self.tInPaths)
  local uPath = Pg.GetGuidByName(self.tInPaths[i])
  local res, x, y, z, yaw = MrxUtil.FindSpawnPointOutOfView(uPath, fDist)
  if not res then
    res, x, y, z, yaw = Pg.GetDistantSpawnPointOnPath(uPath, Pg.GetGuidByName("refinery_office02"), 0, fDist)
  end
  local uVehicle = Pg.Spawn(tVehInfo.template, x, y, z, yaw, false, true)
  local tEvents = {}
  tEvents.spawnDelay = Event.Create(Event.ObjectHibernation, {uVehicle, "awake"}, self._HeliReady, {
    self,
    uVehicle,
    tVehInfo,
    fCallback
  })
  self.tVehicleEvents[uVehicle] = tEvents
  return uVehicle
end

function HeliAttack:_HeliReady(uVehicle, tVehInfo, fCallback)
  local uDriver = Vehicle.GetDriver(uVehicle)
  local tEvents = self.tVehicleEvents[uVehicle]
  tEvents.driverDeath = Event.Create(Event.ObjectDeath, {uDriver}, self._VehicleDestroyed, {self, uVehicle})
  tEvents.driverJacked = Event.Create(Event.ObjectInSeat, {
    uDriver,
    uVehicle,
    "d",
    "x"
  }, self._VehicleDestroyed, {self, uVehicle})
  fCallback(self, uVehicle)
end

function HeliAttack:_StartOrbit(uVehicle)
  local uDriver = Vehicle.GetDriver(uVehicle)
  local i = math.randi(1, table.getn(self.tLoopPaths))
  local uPath = Pg.GetGuidByName(self.tLoopPaths[i])
  local fSpeed = math.randf(0.6, 0.8)
  local tEvents = self.tVehicleEvents[uVehicle]
  if tEvents.aiGoal then
    Ai.RemoveGoal({
      AIGuid = uDriver,
      Handle = tEvents.aiGoal
    })
  end
  tEvents.aiGoal = Ai.Goal({
    AIGuid = uDriver,
    Goal = "PathMove",
    Target = uPath,
    Haste = fSpeed,
    Start = "Nearest",
    Mode = "Loop",
    Priority = "HiPri"
  })
  table.insert(self.tOrbiters, uVehicle)
end

function HeliAttack:_StartStrafe(uVehicle)
  local uDriver = Vehicle.GetDriver(uVehicle)
  local tPlayers = Player.GetAllPlayers()
  local i = Math.randi(1, #tPlayers)
  local tEvents = self.tVehicleEvents[uVehicle]
  if tEvents.aiGoal then
    Ai.RemoveGoal({
      AIGuid = uDriver,
      Handle = tEvents.aiGoal
    })
  end
  tEvents.aiGoal = Ai.Goal({
    AIGuid = uDriver,
    Goal = "Attack",
    Target = Player.GetCharacter(tPlayers[i])
  })
  self.uAttacker = uVehicle
  local fTime = Math.randf(13, 15)
  Event.Delete(self.eStrafeTimer)
  self.eStrafeTimer = Event.Create(Event.TimerRelative, {fTime}, self._ReturnAttackerToOrbit, {self})
end

function HeliAttack:_VehicleDestroyed(uVehicle)
  self:_CleanupOne(self.tVehicleEvents[uVehicle])
  self.tVehicleEvents[uVehicle] = nil
  if self.uAttacker == uVehicle then
    self.uAttacker = nil
    self:_DelayStrafe(2, 3)
  else
    local idx
    for i, uGuid in ipairs(self.tOrbiters) do
      if uGuid == uVehicle then
        idx = i
        break
      end
    end
    table.remove(self.tOrbiters, idx)
  end
  self:_Spawn(vehInfo.heli, 290, HeliAttack._StartOrbit)
  self.nCount = self.nCount + 1
  if self.nCount == 6 then
    self:_Spawn(vehInfo.heli, 270, HeliAttack._StartOrbit)
  end
  self.parent.curWaveBonus = self.parent.curWaveBonus + 10
end

function HeliAttack:_DelayStrafe(fMinTime, fMaxTime)
  local fTime = Math.randf(fMinTime, fMaxTime)
  Event.Delete(self.eStrafeTimer)
  self.eStrafeTimer = Event.Create(Event.TimerRelative, {fTime}, function()
    local uHeli = table.remove(self.tOrbiters, 1)
    self:_StartStrafe(uHeli)
  end)
end

function HeliAttack:_ReturnAttackerToOrbit()
  local uHeli = self.uAttacker
  self.uAttacker = nil
  self:_StartOrbit(uHeli)
  self:_DelayStrafe(15, 17)
end

function AttackWaves:SpawnApc(sVehicleType, iPathSet, fDelay)
  if self.curApc then
    self.curApc:Cleanup()
    self.curApc = nil
  end
  local tConfig = self.tAPCInfo[iPathSet]
  tConfig.veh = vehInfo[sVehicleType]
  self.curApc = APCWave:Create(self.parent, tConfig, fDelay)
  self:ProcessNextCmd()
end

APCWave = {}

function APCWave:Create(parent, tConfig, fInitTime)
  if not tConfig then
    Debug.Printf("APCWave:Create - no configuration supplied")
    return
  end
  local o = {}
  setmetatable(o, self)
  self.__index = self
  o.parent = parent
  o.tConfig = tConfig
  o.tEvents = {}
  o:DelayedSpawn(nil, fInitTime)
  return o
end

function APCWave:Cleanup()
  if self.squadDeathObj then
    self.squadDeathObj:Cancel()
    self.squadDeathObj = nil
  end
  for k, e in pairs(self.tEvents) do
    Event.Delete(e)
  end
  self.tEvents = nil
  self.tPaths = nil
end

function APCWave:Spawn()
  if self.squadDeathObj then
    self.squadDeathObj:Cancel()
    self.squadDeathObj = nil
  end
  local tPathData = self.tConfig
  local res, x, y, z, yaw = MrxUtil.FindSpawnPointOutOfView(Pg.GetGuidByName(tPathData.inPath), 230)
  if not res then
    self:DelayedSpawn(nil, 10)
    return
  end
  local apc = Pg.Spawn(tPathData.veh.template, x, y, z, yaw, false, true)
  local tApcDropData = {
    uVehicle = apc,
    inDest = tPathData.inPath,
    inDestType = "path",
    inSpeed = tPathData.veh.speed * 0.8,
    outDest = tPathData.outPath,
    outDestType = "path",
    outSpeed = tPathData.veh.speed,
    squadName = tPathData.squadName,
    squadTarget = tPathData.defensePoint,
    squadOrder = "attack",
    fDropDoneCallback = function(aVehGuid, aPassengers)
      APCWave.DropDone(self, aVehGuid, aPassengers)
    end
  }
  local e = Event.Create(Event.ObjectHibernation, {apc, "awake"}, function(tData)
    MrxApcDrop:Create(tData)
  end, {tApcDropData})
  self.tEvents[apc] = Event.Create(Event.ObjectDeath, {apc}, APCWave.DelayedAPCSpawn, {self})
end

function APCWave:DropDone(aVehGuid, aPassengers)
  if self.tEvents then
    Event.Delete(self.tEvents[aVehGuid])
    self.tEvents[aVehGuid] = nil
    if self.squadDeathObj == nil then
      self.squadDeathObj = self.parent:CreateChild({
        sName = "INTERNAL OBJ: apc passenger death",
        sModuleName = "MrxTaskObjectiveDestroy",
        vTgtInclude = aPassengers,
        nQuota = table.getn(aPassengers) - 1,
        bDspDescPda = false,
        bDspBlp = false,
        bDspMsg = false,
        sDspShortDesc = "APC Drop",
        fOnComplete = function()
          local tUnits = Ai.Squad({
            Squad = "oc001.vzapcsquad",
            Action = "GetUnits"
          })
          Ai.Squad({
            Squad = "oc001.vzapcsquad",
            Action = "RemoveSquad"
          })
          self:DelayedSpawn()
        end
      })
    end
  end
  Event.Create(Event.ObjectHibernation, {aVehGuid, "hibernated"}, Object.Remove, {aVehGuid})
end

function APCWave:DelayedSpawn(aDeadAPC, spawnTime)
  if aDeadAPC then
    self.tEvents[aDeadAPC] = nil
  end
  spawnTime = spawnTime or Math.randi(7, 9)
  Debug.Printf("\t\t\t\t***** NEW APC SPAWN in : ", spawnTime)
  self.tEvents.eAPCSpawnTimer = Event.Create(Event.TimerRelative, {spawnTime}, self.Spawn, {self})
end

Executive = {}

function Executive:Create(parent, guid, fOnDeath, uRetryPoint)
  local o = {}
  setmetatable(o, self)
  self.__index = self
  o.parent = parent
  o.guid = guid
  self.eDeath = Event.Create(Event.ObjectDeath, {guid}, fOnDeath)
  if uRetryPoint then
    Object.SetTransformToObject(guid, uRetryPoint)
    
    function o:fDehibernateAction()
      self:Stand()
      Ai.SetState({
        AIGuid = self.guid,
        State = "Pacifist",
        Value = true
      })
      self.fDehibernateAction = nil
    end
  else
    function o:fDehibernateAction()
      self:Cower()
      
      Ai.SetState({
        AIGuid = self.guid,
        State = "Pacifist",
        Value = true
      })
      self.fDehibernateAction = nil
    end
  end
  o:OnHibernate()
  return o
end

function Executive:Start(tConfig)
  self._tConfig = tConfig
  self.curProgress = 0
  self.thirdVO = nil
  self.twothirdVO = nil
  self.onemoreVO = nil
  self.hAiCurGoal = Ai.Goal({
    AIGuid = self.guid,
    Goal = "MoveTo",
    Target = Pg.GetGuidByName(self._tConfig.returnPoint),
    Haste = 0.6,
    Priority = "hiPri",
    Callback = self.EnterBuilding,
    CallbackData = {self}
  })
  self.eTimeout = Event.Create(Event.TimerRelative, {10}, self.EnterBuilding, {self})
end

function Executive:EnterBuilding()
  Event.Delete(self.eTimeout)
  Ai.RemoveGoal({
    AIGuid = self.guid,
    Handle = self.hAiCurGoal
  })
  Object.SetTransformToObject(self.guid, Pg.GetGuidByName(self._tConfig.hidePoint))
  local uOffice = Pg.GetGuidByName(self._tConfig.office)
  self.defendObj = self.parent:CreateChild({
    sName = "oc001 defend office",
    sModuleName = "MrxTaskObjectiveProtect",
    vTgtInclude = uOffice,
    bDspBlp = true,
    nSortOrder = 3,
    sDspShortDesc = "[OilCon001.Objectives.defend]",
    fOnComplete = function()
      self._tConfig.fOnComplete(self.parent)
    end,
    fOnCancel = function()
      Object.Kill(self.guid)
    end
  })
  MrxUtil.DisplayHealthBar(self.parent, uOffice, Object.GetHealth(uOffice), false, false)
  self:BurnBox()
end

function Executive:ExitBuilding()
  self.defendObj:Complete()
  local uPoint = Pg.GetGuidByName(self._tConfig.dropoffPoint)
  self:FaceTarget(uPoint)
  Object.SetTransformToObject(self.guid, uPoint)
end

function Executive:BurnBox(iGuid, iState)
  self._curAction = "BurnBox"
  self._curActionState = iState
  self.eTimeout = Event.Create(Event.TimerRelative, {12}, self.EvaluateProgress, {self})
end

function Executive:EvaluateProgress()
  self._curAction = "EvaluateProgress"
  self._curActionState = nil
  local res = self:BoxCompleted()
  if not res then
    self:BurnBox()
  else
    MrxUtil.StopHealthBar(Pg.GetGuidByName(self._tConfig.office))
    self:ExitBuilding()
  end
end

function Executive:BoxCompleted()
  local oParent = self.parent
  self.curProgress = self.curProgress + self._tConfig.boxVal + oParent.curWaveBonus
  oParent.curWaveBonus = 0
  local retval = false
  if self.curProgress >= self._tConfig.goalVal then
    self.curProgres = self._tConfig.goalVal
    retval = true
  elseif self.curProgress > 0.33 * self._tConfig.goalVal and not self.thirdVO then
    self.thirdVO = true
    oParent:_PlayVo(self.guid, self._tConfig.tVO[1])
  elseif self.curProgress > 0.66 * self._tConfig.goalVal and not self.twothirdVO then
    self.twothirdVO = true
    oParent:_PlayVo(self.guid, self._tConfig.tVO[2])
  elseif self.curProgress >= self._tConfig.goalVal - self._tConfig.boxVal and not self.onemoreVO then
    self.onemoreVO = true
    oParent:_PlayVo(self.guid, self._tConfig.tVO[3])
  end
  return retval
end

function Executive:UpdateDisplay()
  local prog = math.floor(100 * self.curProgress / self._tConfig.goalVal)
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 1,
    sText = "[OilCon001.Objectives.filesBurned][objt][yellow][bar" .. prog .. "]"
  })
end

function Executive:CleanupDisplay()
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
end

function Executive:FaceTarget(iTargetGuid, fCallback, uAiGuid, iAiState)
  if iAiState ~= 1 then
  end
  self.hAiCurGoal = Ai.Goal({
    AIGuid = self.guid,
    Goal = "Face",
    Target = {
      Object.GetPosition(iTargetGuid)
    },
    Position = true,
    Priority = "hiPri",
    Callback = fCallback,
    CallbackData = {self}
  })
end

function Executive:Cower()
  Human.DoAction(self.guid, "Cower")
  Ai.Anchor({
    AIGuid = self.guid,
    AnchorRadius = 0
  })
  Ai.Goal({
    AIGuid = self.guid,
    Goal = "Idle",
    Priority = "HiPri"
  })
end

function Executive:Stand(uTarget)
  Human.DoAction(self.guid, "Stand")
  Ai.Anchor({
    AIGuid = self.guid,
    AnchorRadius = 501
  })
  if uTarget then
    Event.Create(Event.HumanActionComplete, {
      self.guid
    }, self.FaceTarget, {self, uTarget})
  end
end

function Executive:Cleanup()
  local res = Ai.RemoveGoal({
    AIGuid = self.guid,
    Handle = self.hAiCurGoal
  })
  if self._tConfig and self._tConfig.office then
    MrxUtil.StopHealthBar(Pg.GetGuidByName(self._tConfig.office))
  end
  self.hAiCurGoal = nil
  Event.Delete(self.eHibernation)
  Event.Delete(self.eTimeout)
  Event.Delete(self.eDeath)
end

function Executive:OnHibernate()
  self.eHibernation = Event.Create(Event.ObjectHibernation, {
    self.guid,
    "awake"
  }, self.OnDehibernate, {self})
end

function Executive:OnDehibernate()
  Human.DisableWeapons(self.guid)
  if self.fDehibernateAction then
    self:fDehibernateAction()
  end
  self.eHibernation = Event.Create(Event.ObjectHibernation, {
    self.guid,
    "hibernated"
  }, self.OnHibernate, {self})
end

function _MoveOCSquad(sSquad, sLoc)
  local uSquad = Pg.GetGuidByName(sSquad)
  local uDefensePoint = Pg.GetGuidByName(sLoc)
  local tSquad = Ai.Squad({SquadGuid = uSquad, Action = "GetUnits"})
  for i, uGuid in pairs(tSquad) do
    if Object.IsAlive(uGuid) then
      Ai.Anchor({
        AIGuid = uGuid,
        AnchorGuid = uDefensePoint,
        AnchorRadius = 30
      })
    end
  end
  local res = Ai.Squad({
    SquadGuid = uSquad,
    Action = "AddCommand",
    Goal = "MoveWithinBoundary",
    Target = {
      Object.GetPosition(uDefensePoint)
    },
    Radius = 10,
    Style = "defend",
    Priority = "HiPri"
  })
end

function _MoveOCBoatSoldier(bReverse)
  local uRunner = Pg.GetGuidByName("oc001.oc.boatrunner")
  local uVeh = Object.InVehicle(uRunner)
  if uVeh then
    Vehicle.Exit(uVeh, uRunner)
  end
  local res = Ai.Goal({
    AIGuid = uRunner,
    Goal = "PathMove",
    Target = Pg.GetGuidByName("oc001.pth.ship_patrol"),
    Haste = 0.9,
    Reverse = bReverse,
    Priority = "HiPri",
    Force = true
  })
end

function _MoveSite3OCSquad()
  local res = Ai.Squad({
    SquadGuid = Pg.GetGuidByName("oc001.obj1.ocsquad2"),
    Action = "AddCommand",
    Goal = "MoveWithinBoundary",
    Target = {
      Object.GetPosition(uDefensePoint)
    },
    Radius = 10,
    Style = "defend",
    Priority = "HiPri",
    Callback = function(uGuid)
      local h = Ai.Anchor({
        AIGuid = uGuid,
        AnchorRadius = 30,
        AnchorGuid = uDefensePoint
      })
    end
  })
end

function _OCSavedBanter(self)
  local function banter2(uExcludeGuid)
    self:_PerformBanter("oc001.obj1.ocsquad1", "OCMerc-Briefing-Contract-Oil01-68", uExcludeGuid)
  end
  
  self:_PerformBanter("oc001.obj1.ocsquad1", "OCMerc-Briefing-Contract-Oil01-66", nil, banter2)
end

function _PerformBanter(self, sSquadName, sVOLine, uExcludeGuid, fCallback)
  local tSquad = Ai.Squad({
    SquadGuid = Pg.GetGuidByName(sSquadName),
    Action = "GetUnits"
  })
  for i, uGuid in pairs(tSquad) do
    if uGuid ~= uExcludeGuid and Object.IsAlive(uGuid) then
      self:_PlayVo(uGuid, sVOLine, fCallback, {uGuid})
      return
    end
  end
end

function _DestroyGate(sName)
  local gate = Pg.GetGuidByName(sName)
  Event.Create(Event.ObjectHibernation, {gate, "awake"}, function()
    Object.Kill(gate)
    local x, y, z = Object.GetPosition(gate)
    Pg.Spawn("fx_Explosion_Huge", x, y, z, false, false)
  end)
end

function _StagingSetup(self)
  _DestroyGate("_ocoutpost_wallgate 0x10001d5d")
  local uJeep = _StagingSpawnVehicle("M151 .50Cal (VZ) (DriverGunner)", "oc001.loc.staging.spawn1", "oc001.pth.staging.in1", 0.7)
  Object.SetHealth(uJeep, 8)
  local uGuid = Pg.GetGuidByName("oc001.loc.stagedEnc")
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    uGuid,
    "<",
    150,
    false,
    false
  }, _StagingBeginSequence, {self})
end

function _StagingBeginSequence(self)
  local uJeep = _StagingSpawnVehicle("M151 .50Cal (VZ) (DriverGunner)", "oc001.loc.staging.spawn2", "oc001.pth.staging.in2", 0.8)
  Object.SetHealth(uJeep, 8)
  local uEXT = _StagingSpawnVehicle("EXT (DriverGunner)", "oc001.loc.staging.spawn3", "oc001.pth.staging.in3", 0.6)
  Object.SetHealth(uEXT, 12)
  local uGuid = Pg.GetGuidByName("oc001.loc.stagedEnc")
  local res = self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    uGuid,
    "<",
    15,
    false,
    false
  }, function()
    local uExp = Pg.GetGuidByName("oc001.loc.stagedExp")
    MrxUtil.SpawnObject("fx_Explosion_Large", uExp)
  end)
end

function _StagingSpawnVehicle(sTemplate, sLoc, sPath, fSpeed)
  local uGuid = MrxUtil.SpawnObject(sTemplate, sLoc)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, _StagingStartVehicle, {
    uGuid,
    Pg.GetGuidByName(sPath),
    fSpeed
  })
  return uGuid
end

function _StagingStartVehicle(uVehicle, uPath, fSpeed)
  local uDriver = Vehicle.GetDriver(uVehicle)
  local moveGoal = Ai.Goal({
    AIGuid = uDriver,
    Goal = "PathMove",
    Target = uPath,
    Haste = fSpeed,
    Priority = "HiPri"
  })
end

function _RabbitSetup(self)
  local res = self:_CreateEvent(Event.Boundary, {
    self.exec.guid,
    Pg.GetGuidByName("oc001.rgn.warehouse01"),
    "exit",
    false
  }, _RabbitSpawn)
end

function _RabbitSpawn()
  local uCar = MrxUtil.SpawnObject("EXT (DriverGunner)", "oc001.rabbitcar")
  Event.Create(Event.ObjectHibernation, {uCar, "awake"}, _RabbitStart, {uCar})
end

function _RabbitStart(uCar)
  local uDriver = Vehicle.GetDriver(uCar)
  local res = not uDriver or Object.IsPlayerControlled(uDriver) or Ai.Goal({
    AIGuid = uDriver,
    Goal = "PathMove",
    Target = Pg.GetGuidByName("oc001.pth.rabbit"),
    Haste = 0.4,
    Priority = "HiPri"
  })
end

function _SetupConvoy1()
  local tConvoyVehicles = {
    "EXT (DriverGunner) 0x000f032e",
    "Guntruck (OC) 0x000f032d",
    "Guntruck (OC) 0x000f032c",
    "Guntruck (OC) 0x000f032b"
  }
  for i, veh in pairs(tConvoyVehicles) do
    local uGuid = Pg.GetGuidByName(veh)
    Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, _SetupConvoy2, {uGuid})
  end
end

function _SetupConvoy2(uGuid)
  local h = Event.Create(Event.ObjectHibernation, {uGuid, "hibernated"}, Object.Remove, {uGuid})
  Debug.Printf("--= convoy cleanup on ", uGuid)
end

function CreateMissionBoundaryDelayed(self, tConfig)
  if self.missionBoundary then
    self.missionBoundary:Cancel()
    self.missionBoundary = nil
  end
  Event.Delete(self._tEvents.eMissionBoundary)
  self._tEvents.eMissionBoundary = Event.Create(Event.TimerRelative, {75}, CreateMissionBoundary, {self, tConfig})
end

function CreateMissionBoundary(self, tConfig)
  tConfig.fRadius = 190
  tConfig.sLabel = "[yellow][OilCon001.Objectives.timer]"
  tConfig.fWarnTime = 15
  tConfig.fFailTime = 30
  tConfig.iTray = 2
  tConfig.fMyCallback = tConfig.fCallback
  tConfig.fCallback = UpdateMissionBoundaryStatus
  tConfig.tCallbackData = {self}
  self.missionBoundary = MrxMissionBoundary:Create(tConfig)
end

function UpdateMissionBoundaryStatus(oMissionBoundary, sStatus, self)
  if sStatus == "fail" and oMissionBoundary._tConfig.fMyCallback then
    oMissionBoundary._tConfig.fMyCallback(self)
  elseif sStatus == "warning" and self._tConfig.sLayer then
    MrxLayerManager.Add(self._tConfig.sLayer)
  end
end

function BuildingDestroyedSetup(self, sBuilding)
  local uGuid = Pg.GetGuidByName(sBuilding)
  self._tEvents[uGuid] = Event.Create(Event.ObjectDeath, {uGuid}, BuildingDestroyedWarning, {self})
end

function BuildingDestroyedWarning(self, uBuilding)
  local bx, by, bz = Object.GetPosition(uBuilding)
  local ex, ey, ez = Object.GetPosition(self.exec.guid)
  local len = Math.Length(bx - ex, 0, bz - ez)
  if len < 20 then
    Object.Kill(self.exec.guid)
  else
    MrxVoSequence.Start({
      {
        "OilExec-In-Mission-Contract-Oil01-24",
        self.exec.guid
      },
      {
        mattias = "Mattias-In-Mission-Contract-Oil01-25",
        jennifer = "Jennifer-In-Mission-Contract-Oil01-26",
        chris = "Chris-In-Mission-Contract-Oil01-27"
      },
      0.5,
      {_MyCancel}
    })
  end
end

function BuildingDestroyedCleanup(self, sBuilding)
  local uGuid = Pg.GetGuidByName(sBuilding)
  Event.Delete(self._tEvents[uGuid])
  self._tEvents[uGuid] = nil
end

function FirstWarehouseDestroyedSetup(self)
  local uGuid = Pg.GetGuidByName("refinery_doc_warehouse01")
  self._tEvents[uGuid] = Event.Create(Event.ObjectDeath, {uGuid}, FirstWarehouseDestroyed, {self})
end

function FirstWarehouseDestroyed(self)
  if Object.InsideBoundary(self.exec.guid, Pg.GetGuidByName("oc001.rgn.warehouse01"), true) then
    Object.Kill(self.exec.guid)
  end
end

ksMissionAmbience = "emt_distant_battle_01"

function SoundRegion_Inside(self)
  self._tEvents.eSoundRgn = Event.Create(Event.Boundary, {
    Player.GetPrimaryCharacter(),
    Pg.GetGuidByName("oc001.rgn.boundary"),
    "exit",
    false
  }, SoundRegion_Outside, {self})
  Sound.CueAmbience(ksMissionAmbience)
end

function SoundRegion_Outside(self)
  self._tEvents.eSoundRgn = Event.Create(Event.Boundary, {
    Player.GetPrimaryCharacter(),
    Pg.GetGuidByName("oc001.rgn.boundary"),
    "enter",
    false
  }, SoundRegion_Inside, {self})
  Sound.StopAmbience(ksMissionAmbience)
end

function NetEventCallback(nEventId, tArgs)
end
