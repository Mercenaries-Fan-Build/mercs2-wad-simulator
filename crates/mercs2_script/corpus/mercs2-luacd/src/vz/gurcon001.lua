inherit("MrxTaskContract")
import("DangerousBuilding")
import("MrxSubtitle")
import("MrxVoSequence")
import("MrxSupportData")
import("Munitions")
import("MrxTransit")
BeachCheckPointEventWest = nil
BeachCheckPointEventEast = nil
BeachCheckPointEventMid = nil
BuildingsDestroyed = 0
tHelosSpawned = {}

function LoadAssets(self, tSaveData)
  Debug.Printf("inside loadassets")
  tLayersToAdd = {
    "Vz_State_GurCon001",
    "Vz_State_GurCon001_staging",
    "vz_state_temp_staging_gurcon002",
    "vz_state_gurcon001_pristine",
    "Vz_State_GurCon001_outpost",
    "Vz_State_GurCon001_outpost_pristine",
    "VZ_State_GurCon001_TG",
    "vz_state_gurcon001_fortress"
  }
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  tLayersToAdd = {
    "vz_state_temp_staging_gurcon002",
    "vz_state_gurcon001_pristine",
    "Vz_State_GurCon001_outpost",
    "Vz_State_GurCon001_outpost_pristine"
  }
end

function Activated(self)
  Debug.Printf("inside and before activated call")
  MrxTaskContract.Activated(self)
  Debug.Printf("Inside Activated")
  ObjectivesDestroyed = 0
  self:_CreateEvent(Event.ObjectHibernation, {
    Player.GetLocalCharacter(),
    "awake"
  }, Start, {self})
end

function Start(self)
  _SetupVO(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("LR_VZGurAttack_GC2"),
    "enter"
  }, _SetupVZAttack)
  if not self:_GetFlag("BeachReached") then
    AddBeachCheckpoints(self)
  else
    Debug.Printf("Deleting Soldiers")
    if Pg.GetGuidByName("VZ Soldier 0x00126c6f") then
      Object.Remove(Pg.GetGuidByName("VZ Soldier 0x00126c6f"))
    end
    if Pg.GetGuidByName("VZ Soldier 0x00126c6b") then
      Object.Remove(Pg.GetGuidByName("VZ Soldier 0x00126c6b"))
    end
  end
  _SetupHeloAttacks(self)
  DestroyCastle(self)
end

function DestroyCastle(self)
  MunitionsCollected = 0
  self:CreateChild({
    sName = "BlowUpCastle",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "_island_bld_fortress01 0x0008a5b0"
    },
    sDspShortDesc = "[GurCon001.objective.destroycastle]",
    vVoSeqOnAdd = {
      "Fiona-Banter-Contract-Gur01-01",
      0.4,
      {
        mattias = "Mattias-Banter-Contract-Gur01-02",
        jennifer = "jennifer-Banter-Contract-Gur01-03",
        chris = "chris-Banter-Contract-Gur01-04"
      },
      1,
      "Fiona-In-Mission-Contract-Gur001-25",
      1,
      "Fiona-In-Mission-Contract-Gur001-48"
    },
    tOnComplete = {
      {
        ObjectiveDestroyed,
        {self}
      }
    }
  })
  self:CreateChild({
    sName = "BlowUpTower",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "_island_bld_tower01 0x0008a5b1"
    },
    sDspShortDesc = "[GurCon001.objective.destroytower]",
    tOnComplete = {
      {
        ObjectiveDestroyed,
        {self}
      }
    }
  })
  self:CreateChild({
    sName = "BlowUpBridge",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "_island_bld_bridge01 0x0008a5af"
    },
    sDspShortDesc = "[GurCon001.objective.destroybridge]",
    tOnComplete = {
      {
        ObjectiveDestroyed,
        {self}
      }
    }
  })
  if self:_GetFlag("BonusCompleted") then
    self:_SetPlayer1Bonus(500000)
    self:_SetPlayer2Bonus(500000)
    KillBarracks(self)
  else
    SetupBonusObj(self)
  end
end

function ObjectiveDestroyed(self)
  ObjectivesDestroyed = ObjectivesDestroyed + 1
  if ObjectivesDestroyed == 1 then
    MrxVoSequence.Start({
      "Fiona.xfio136"
    })
  elseif ObjectivesDestroyed == 2 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Job-Oil11-16"
    })
  elseif ObjectivesDestroyed == 3 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Gur001-50",
      0.5,
      {
        mattias = "Mattias-In-Mission-Contract-Gur001-21",
        jennifer = "jennifer-In-Mission-Contract-Gur001-22",
        chris = "chris-In-Mission-Contract-Gur001-23"
      },
      1,
      "Fiona-In-Mission-Contract-Gur001-24",
      {
        self.Complete,
        {self}
      }
    })
  end
end

function SetupBonusObj(self)
  uBonusObj = self:CreateChild({
    sName = "Destroy Barracks",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "gurcon001.barracks.01",
      "gurcon001.barracks.003",
      "gurcon001.barracks.004",
      "gurcon001.barracks.005",
      "gurcon001.barracks.006",
      "gurcon001.barracks.007",
      "gurcon001.barracks.008",
      "_vzoutpost_bld_barrackbunker 0x000c9a1b"
    },
    sDspShortDesc = "[GurCon001.objective.DestroyVzBarracks]",
    bOptional = true,
    fOnComplete = function()
      MrxVoSequence.Start({
        "Fiona.va6fio06"
      })
      self:_SetPlayer1Bonus(500000)
      self:_SetPlayer2Bonus(500000)
      self:_SetFlag("BonusCompleted")
      _Checkpoint()
      uBonusObj.Complete(uBonusObj)
    end
  })
end

function KillBarracks(self)
  tBarracks = {
    Pg.GetGuidByName("gurcon001.barracks.01"),
    Pg.GetGuidByName("gurcon001.barracks.003"),
    Pg.GetGuidByName("gurcon001.barracks.004"),
    Pg.GetGuidByName("gurcon001.barracks.005"),
    Pg.GetGuidByName("gurcon001.barracks.006"),
    Pg.GetGuidByName("gurcon001.barracks.007"),
    Pg.GetGuidByName("gurcon001.barracks.008"),
    Pg.GetGuidByName("_vzoutpost_bld_barrackbunker 0x000c9a1b")
  }
  for i, uBarrack in ipairs(tBarracks) do
    self:_CreateEvent(Event.ObjectHibernation, {uBarrack, "awake"}, Object.Kill, {uBarrack})
  end
end

function SetUpMunitionsObjective(self)
  oMunitions = self:CreateChild({
    sName = "GurCon001_MasterMunitions",
    sModuleName = "MrxTaskObjective",
    sDspShortDesc = "[GurCon001.objective.munition]",
    nQuota = 3,
    bOptional = true
  })
  local tMunitions = {
    "Munitions (Rocket Artillery) 0x0010565d",
    "Munitions (Rocket Artillery) 0x0010565e",
    "Munitions (Rocket Artillery) 0x00105661"
  }
  uSubMunitions = {}
  for i, sObject in pairs(tMunitions) do
    Debug.Printf("Adding " .. sObject .. " subObjective")
    local uObject = Pg.GetGuidByName(sObject)
    if uObject then
      Munitions.ClearStatus(uObject)
      uSubMunitions[uObject] = self:CreateChild({
        sName = "GurCon001_SubMunitions" .. i,
        sModuleName = "MrxTaskObjectiveProtect",
        sDspShortDesc = "[GurCon001.objective.munitions]",
        vDestLoc = uObject,
        vTgtInclude = uObject,
        bOptional = true,
        sDspBlpRdrIcon = "objective_action",
        sDspBlpWldIcon = "HUD_objective_action",
        sDspBlpPdaIcon = "icon_action_2_mc",
        tOnCancel = {
          {
            _DestroyedMunitions,
            {self, uGuid}
          }
        }
      })
    end
  end
  self:_CreatePersistentEvent(Event.ScriptEvent, {
    "MunitionsPickup",
    function(tData)
      return Pg.GetGuidByName("Munitions (Rocket Artillery) 0x00105661") == tData[2] or Pg.GetGuidByName("Munitions (Rocket Artillery) 0x0010565e") == tData[2] or Pg.GetGuidByName("Munitions (Rocket Artillery) 0x0010565d") == tData[2]
    end
  }, _CompletedMunitions, {self})
end

function _MissionComplete(self)
  MrxVoSequence.Start({
    {
      mattias = "Mattias-In-Mission-Contract-Gur001-21",
      jennifer = "jennifer-In-Mission-Contract-Gur001-22",
      chris = "chris-In-Mission-Contract-Gur001-23"
    },
    1,
    "Fiona-In-Mission-Contract-Gur001-24",
    {
      self.Complete,
      {self}
    }
  })
end

function _SetupVZAttack()
  tGoalParamsJeep1 = {
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("M151 .50Cal (VZ) (Full) 0x000dc48d")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Path_VZGurAttackJeep2_GC2"),
    Priority = "hiPri",
    Haste = 1
  }
  Ai.Goal(tGoalParamsJeep1)
  tGoalParamsJeep2 = {
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("M151 .50Cal (VZ) (Full) 0x000dc48e")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Path_VZGurAttackJeep1_GC2"),
    Priority = "hiPri",
    Haste = 1
  }
  Ai.Goal(tGoalParamsJeep2)
  tGoalParamsTruck = {
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("M35 (Cargo) (VZ) (Full) 0x000dc48c")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Path_VZGurAttackTruck_GC2"),
    Priority = "hiPri",
    Haste = 1,
    Callback = Ai.Deploy,
    CallbackData = {
      {
        Vehicle = Pg.GetGuidByName("M35 (Cargo) (VZ) (Full) 0x000dc48c"),
        Priority = "hiPri"
      }
    }
  }
  Ai.Goal(tGoalParamsTruck)
end

function _SpawnVehicleOutOfView(self, sPath, sVehicle, sMode)
  bSuccess, x, y, z, nFacing = MrxUtil.FindSpawnPointOutOfView(Pg.GetGuidByName(sPath), 290)
  if bSuccess == true then
    SPAWN = Pg.Spawn(sVehicle, x, y, z, nFacing, false, true)
    self:_CreateEvent(Event.ObjectHibernation, {
      Vehicle.GetDriver(SPAWN),
      "awake"
    }, function()
      Ai.Goal({
        AIGuid = Vehicle.GetDriver(SPAWN),
        Goal = "PathMove",
        Target = Pg.GetGuidByName(sPath),
        Priority = "loPri",
        Start = "Nearest",
        Mode = sMode
      })
    end)
    table.insert(tHelosSpawned, SPAWN)
  else
  end
end

function _CompletedMunitions(self, tData)
  local uObjGuid = tData[2]
  oMunitions:CompletePart()
  if uSubMunitions[uObjGuid] then
    uSubMunitions[uObjGuid]:Complete()
  end
end

function _DestroyedMunitions(self, uGuid)
  nQuota = nQuota or 3
  nQuota = nQuota - 1
  Debug.Printf("Changed quota to " .. nQuota)
  if nQuota < 1 then
    oMunitions:Cancel()
  else
    oMunitions.Configure({nQuota = nQuota})
  end
end

function _SetupVO(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("LRGurcon001Shore"),
    "enter"
  }, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Gur001-29"
    })
  end)
  self:_CreateEvent(Event.TimerRelative, {45}, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Gur001-41"
    })
  end)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("LR_CastleGR2"),
    "enter"
  }, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-Contract-Gur001-32"
    }
  })
  self:_CreateEvent(Event.ObjectDeath, {
    GetGuidByName("M113 Jammer (VZ) (Driver) 0x000f3005")
  }, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-Contract-Gur01-08"
    }
  })
end

function AddBeachCheckpoints(self)
  BeachCheckPointEventWest = self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("GurCon001_Checkpoint"),
    "enter"
  }, function()
    self:_SetFlag("BeachReached")
    _Checkpoint({
      "Loc_CheckPoint_WestBeachP1",
      "Loc_CheckPoint_WestBeachP2"
    })
  end)
end

function _SetupHeloAttacks(self)
  oCallHeloJammer1 = self:_CreateEvent(Event.ObjectDeath, {
    GetGuidByName("M113 Jammer (VZ) (Driver) 0x00126caa")
  }, function()
    _SpawnVehicleOutOfView(self, "Path 0x00126cb6", "Alouette3 Attack (VZ) (Driver)", "Oneway")
    if oCallHeloJammer2 then
      Event.Delete(oCallHeloJammer2)
    end
  end)
  oCallHeloJammer2 = self:_CreateEvent(Event.ObjectDeath, {
    GetGuidByName("M113 Jammer (VZ) (Driver) 0x00126ca9")
  }, function()
    _SpawnVehicleOutOfView(self, "Path 0x00126cb6", "Alouette3 Attack (VZ) (Driver)", "Oneway")
    if oCallHeloJammer1 then
      Event.Delete(oCallHeloJammer1)
    end
  end)
  oCallHeloCastle1 = self:_CreateEvent(Event.ObjectDeath, {
    GetGuidByName("_island_bld_fortress01 0x0008a5b0")
  }, function()
    _SpawnVehicleOutOfView(self, "Path 0x00126c94", "Alouette3 Attack (VZ) (Driver)", "Oneway")
    if oCallHeloCastle2 then
      Event.Delete(oCallHeloCastle2)
    end
    if oCallHeloCastle3 then
      Event.Delete(oCallHeloCastle3)
    end
  end)
  oCallHeloCastle2 = self:_CreateEvent(Event.ObjectDeath, {
    GetGuidByName("_island_bld_bridge01 0x0008a5af")
  }, function()
    _SpawnVehicleOutOfView(self, "Path 0x00126c94", "Alouette3 Attack (VZ) (Driver)", "Bounce")
    if oCallHeloJammer1 then
      Event.Delete(oCallHeloCastle1)
    end
    if oCallHeloCastle3 then
      Event.Delete(oCallHeloCastle3)
    end
  end)
  oCallHeloCastle3 = self:_CreateEvent(Event.ObjectDeath, {
    GetGuidByName("_island_bld_tower01 0x0008a5b1")
  }, function()
    uJammer = Pg.GetGuidByName("M113 Jammer (VZ) (Driver) 0x00126ca9")
    nX, nY, nZ = Object.GetPosition(uJammer)
    if Pg.IsPointInBoundary(nX, nY, nZ, Pg.GetGuidByName("LR_TowerBase")) then
      Object.Kill(uJammer)
    end
    _SpawnVehicleOutOfView(self, "Path 0x00126c94", "Alouette3 Attack (VZ) (Driver)", "Bounce")
    if oCallHeloCastle1 then
      Event.Delete(oCallHeloCastle1)
    end
    if oCallHeloCastle2 then
      Event.Delete(oCallHeloCastle2)
    end
  end)
end

function Cleanup(self)
  Debug.Printf("Inside Cleanup")
  Sound.SetDynamicMusic(true)
  if oCastleMusicOff then
    Event.Delete(oCastleMusicOff)
  end
  if oCastleMusicOn then
    Event.Delete(oCastleMusicOn)
  end
  for i, sLayerName in ipairs(tLayersToAdd) do
    MrxLayerManager.MarkForRemoval(sLayerName)
  end
  for i, uHeloGuid in ipairs(tHelosSpawned) do
    if uHeloGuid then
      local P1Vehicle = Vehicle.GetFromRider(Player.GetPrimaryCharacter())
      local P2Vehicle = Vehicle.GetFromRider(Player.GetSecondaryCharacter())
      if P1Vehicle ~= uHeloGuid and P2Vehicle ~= uHeloGuid then
        Object.Remove(uHeloGuid)
      end
    end
  end
  MrxTaskContract.Cleanup(self)
end
