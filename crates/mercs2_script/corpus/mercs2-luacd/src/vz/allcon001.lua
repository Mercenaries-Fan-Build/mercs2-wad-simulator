import("MrxSubtitle")
inherit("MrxTaskContract")
import("MrxVoSequence")
import("MrxSupportData")

function LoadAssets(self, tSaveData)
  MrxLayerManager.Remove({
    "vz_state_Margarita_precrash"
  }, function()
    local tLayersToAdd = {
      "Vz_State_AllCon001",
      "vz_State_Margarita_crash",
      "vz_State_AllCon001_pristine"
    }
    MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  end)
end

function Activated(self)
  nVipSaved = 0
  nPlaneParts = 3
  uFoundVIP2 = 0
  MrxTaskContract.Activated(self)
  ObjectivePlane1(self)
  ObjectiveTalkToVIP1(self)
  ObjectiveTalkToVIP2(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("reg_MargaritaChinaFactionZone"),
    "enter",
    false
  }, TalkAboutVIP1, {self})
  uVIP2Event = self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Civ_VIP_2"),
    "<",
    20,
    false,
    false
  }, ShowVIP2, {self})
  self:_CreateEvent(Event.ObjectHibernation, {
    Pg.GetGuidByName("patrolBoat_west2"),
    "awake"
  }, BoatPatrol, {
    self,
    "patrolBoat_west2",
    "Pa_patrolBoat1"
  })
  self:_CreateEvent(Event.ObjectHibernation, {
    Pg.GetGuidByName("patrolBoat_2"),
    "awake"
  }, BoatPatrol, {
    self,
    "patrolBoat_2",
    "Pa_BoatPatrol2"
  })
  self:_CreateEvent(Event.ObjectHibernation, {
    Pg.GetGuidByName("patrolBoat_3"),
    "awake"
  }, BoatPatrol, {
    self,
    "patrolBoat_3",
    "Pa_BoatPatrol2"
  })
  self:_CreateEvent(Event.ObjectHibernation, {
    Pg.GetGuidByName("patrolBoat_4"),
    "awake"
  }, BoatPatrol, {
    self,
    "patrolBoat_4",
    "Pa_BoatPatrol_4"
  })
end

function RelationSetup(self)
  Debug.Printf("SSSSSSSSSSSSSSSSSSSSSSS   Set VIPs as neutral")
  Ai.SetRelation(GetGuidByName("China"), Pg.GetGuidByName("Civ_VIP_1"), 100)
  Ai.SetRelation(Pg.GetGuidByName("Civ_VIP_1"), GetGuidByName("China"), 100)
  for i = 1, 4 do
    Ai.SetRelation(GetGuidByName("China"), Pg.GetGuidByName("jail_11_" .. i), 100)
    Ai.SetRelation(Pg.GetGuidByName("jail_11_" .. i), GetGuidByName("China"), 100)
  end
end

function BoatPatrol(self, sVeh, sPath)
  Debug.Printf("HhhhMMMMMMMMMMMmmmmm" .. sVeh .. "IS awake")
  local uDriver = Vehicle.GetDriver(Pg.GetGuidByName(sVeh))
  local uPath = Pg.GetGuidByName(sPath)
  if uDriver then
    Debug.Printf("told to move" .. sVeh)
    local tGoalParams = {
      AIGuid = uDriver,
      Goal = "PathMove",
      Target = uPath,
      Start = "Nearest",
      Priority = "medPri",
      Mode = "Bounce"
    }
    self:_CreateEvent(Event.TimerRelative, {1}, Ai.Goal, {tGoalParams})
  end
end

function ObjectivePlane1(self)
  tInitialVOTable = {
    "Fiona-In-Mission-Contract-All02-42",
    {
      mattias = "Mattias-In-Mission-Contract-All02-43",
      jennifer = "Jennifer-In-Mission-Contract-All02-44",
      chris = "Chris-In-Mission-Contract-All02-45"
    },
    "Fiona-In-Mission-Contract-All01-40",
    "Fiona-In-Mission-Contract-All01-45"
  }
  oPlaneNose = self:CreateChild({
    sName = "Destroy the plane's nose",
    sModuleName = "MrxTaskObjectiveDestroy",
    vVoSeqOnAdd = tInitialVOTable,
    vTgtInclude = {
      "ruinsplane_front_AllCon1",
      "ruinsplane_mid_AllCon1",
      "ruinsplane_back_AllCon1"
    },
    sDspShortDesc = "[AllCon001.Objectives.001]",
    tOnPartComplete = {
      {
        PartDestroyed,
        {self}
      }
    },
    fOnComplete = function()
      if nVipSaved > 0 then
        local nVipBonus = nVipSaved * 3000000
        if Net.IsActive() then
          self:_SetPlayer1Bonus(nVipBonus)
          self:_SetPlayer2Bonus(nVipBonus)
        else
          self:_SetPlayer1Bonus(nVipBonus)
        end
      end
      self:_CreateEvent(Event.TimerRelative, {4}, self.Complete, {self})
    end,
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function PartDestroyed(self)
  nPlaneParts = nPlaneParts - 1
  if nPlaneParts == 2 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All01-22",
      "Fiona-In-Mission-Contract-All01-23"
    })
  elseif nPlaneParts == 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All01-24",
      "Fiona-In-Mission-Contract-All01-25"
    })
  elseif nPlaneParts == 0 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All01-29",
      "Fiona-In-Mission-Contract-All01-27"
    })
  end
end

function PlayObjectiveMusic(self)
  if nPlaneParts > 1 then
    PlayThreatMusic(self, 50)
  elseif nPlaneParts == 1 then
    MrxMusic.PlaySpecialMusic("mu_fac_an_kickass_01")
    self:_CreateEvent(Event.TimerRelative, {50}, MrxMusic.StopSpecialMusic, {})
    self:_CreateEvent(Event.TimerRelative, {50.3}, PlayThreatMusic, {self, 20})
  end
end

function TalkAboutVIP1(self)
  if not self:_GetFlag("MargaritaReached") then
    self:_SetFlag("MargaritaReached")
    _Checkpoint({
      "LocAll001ckpt1_p1",
      "LocAll001ckpt1_p2"
    })
  end
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-All01-21",
    "Fiona-In-Mission-Contract-All01-48",
    {
      mattias = "mattias-In-Mission-Contract-All01-52",
      jennifer = "jennifer-In-Mission-Contract-All01-53",
      chris = "chris-In-Mission-Contract-All01-54"
    }
  })
end

function PlayThreatMusic(self, nEndTime)
  Sound.SetActionLevelsMusic(3, 0, 0, 0)
  Sound.LockActionLevelMusic(true)
  self:_CreateEvent(Event.TimerRelative, {nEndTime}, Sound.LockActionLevelMusic, {false})
end

function ShowVIP1(self)
  oVIP1talk:Configure({bDsp = true})
end

function ObjectiveTalkToVIP1(self)
  oVIP1talk = self:CreateChild({
    sName = "TalktoVIP1",
    sModuleName = "MrxTaskObjectiveRelease",
    sActionLabel = "[ContextAction.ReleasePrisoner]",
    vTgtInclude = "Civ_VIP_1",
    bOptional = true,
    sDspShortDesc = "[AllCon001.Objectives.004]",
    tOnPartComplete = {
      {
        ObjectiveVIP1,
        {self}
      }
    },
    fOnCancel = function()
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-All01-42"
      })
    end
  })
  local nSite = 11
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Civ_VIP_1"),
    "<",
    10,
    false,
    false
  }, MoveCivs, {self, nSite})
end

function ObjectiveVIP1(self, uTalked)
  oVIP1extract = self:CreateChild({
    sName = "Extract VIP 1",
    sModuleName = "MrxTaskObjectiveExtract",
    vTgtInclude = "Civ_VIP_1",
    bOptional = true,
    sDspShortDesc = "[AllCon001.Objectives.007]",
    uStartAttachedToPlayer = uTalked,
    bStop = false,
    bXZOnly = false,
    fOnComplete = function()
      nVipSaved = nVipSaved + 1
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-All01-37"
      })
      Debug.Printf("Vip 1 Rescue completed ****************************************")
    end,
    fOnCancel = function()
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-All01-41"
      })
    end
  })
  VIP1givesInfo(self)
end

function MoveCivs(self, nSite)
  for i = 1, 4 do
    Debug.Printf("CCCCCCCCCCCCCCCCCCCCCCCC  civie cower power!!!")
    local uCivGoo = Pg.GetGuidByName("jail_" .. nSite .. "_" .. i)
    if uCivGoo then
      Debug.Printf("AAAAAAAAAAAAAAAAAAAAAA and it's done")
      Human.SetState(uCivGoo, "Cower", "Idle")
    end
  end
end

function VIP1givesInfo(self)
  if uFoundVIP2 == 0 then
    Event.Delete(uVIP2Event)
    MrxVoSequence.Start({
      "AlliedSoldier-In-Mission-Contract-All01-08",
      5,
      "AlliedSoldier-In-Mission-Contract-All01-09"
    })
    self:_CreateEvent(Event.TimerRelative, {10}, ShowVIP2, {self})
  end
end

function ShowVIP2(self)
  uFoundVIP2 = 1
  oVIP2talk:Configure({bDsp = true})
end

function ObjectiveTalkToVIP2(self)
  oVIP2talk = self:CreateChild({
    sName = "TalktoVIP2",
    sModuleName = "MrxTaskObjectiveRelease",
    bOptional = true,
    sActionLabel = "[ContextAction.ReleasePrisoner]",
    vTgtInclude = "Civ_VIP_2",
    sDspShortDesc = "[AllCon001.Objectives.005]",
    bDsp = false,
    tOnPartComplete = {
      {
        ObjectiveVIP2,
        {self}
      }
    },
    fOnCancel = function()
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-All01-17"
      })
    end
  })
  local nSite = 22
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Civ_VIP_2"),
    "<",
    10,
    false,
    false
  }, MoveCivs, {self, nSite})
end

function ObjectiveVIP2(self, uTalked)
  MrxVoSequence.Start({
    "AlliedSoldier-In-Mission-Contract-All01-11",
    "AlliedSoldier-In-Mission-Contract-All01-31"
  })
  oVIP2extract = self:CreateChild({
    sName = "Extract VIP 2",
    sModuleName = "MrxTaskObjectiveExtract",
    vTgtInclude = "Civ_VIP_2",
    sDspShortDesc = "[AllCon001.Objectives.006]",
    uStartAttachedToPlayer = uTalked,
    bOptional = true,
    bStop = false,
    bXZOnly = false,
    fOnComplete = function()
      nVipSaved = nVipSaved + 1
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-All01-26"
      })
      Debug.Printf("Vip 2 Rescue completed ****************************************")
    end,
    fOnCancel = function()
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-All01-17"
      })
    end
  })
end

function Cleanup(self)
  MrxTaskContract.Cleanup(self)
end
