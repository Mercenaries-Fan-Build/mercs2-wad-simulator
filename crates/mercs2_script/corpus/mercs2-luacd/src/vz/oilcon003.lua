inherit("MrxTaskContract")
import("MrxFactionManager")
import("MrxVoSequence")

function Activated(self)
  MrxTaskContract.Activated(self)
  nBonusMult = 3
  nDudeHealth = 0
  nVehHealth = 100
  nCompletions = self:GetNumCompletions()
  tInitialVOTable = {
    "Fiona-In-Mission-MinorContract-Oil03-23"
  }
  if nCompletions == 1 then
    table.insert(tInitialVOTable, "Fiona-In-Mission-MinorContract-Oil03-25")
  elseif nCompletions >= 2 then
    table.insert(tInitialVOTable, "Fiona-In-Mission-MinorContract-Oil03-26")
  else
    table.insert(tInitialVOTable, "Fiona-In-Mission-MinorContract-Oil03-24")
  end
  self:CreateChild({
    sName = "TalktoDuder",
    sModuleName = "MrxTaskObjectiveAction",
    sActionLabel = "[ContextAction.Talk]",
    vTgtInclude = "OilCon003_deliv",
    sDspShortDesc = "[OilCon003.Objectives.001]",
    tOnPartComplete = {
      {
        DeliverDude,
        {self}
      }
    },
    vVoSeqOnAdd = tInitialVOTable
  })
  Ai.SetState({
    AIGuid = Pg.GetGuidByName("OilCon003_deliv"),
    State = "Pacifist",
    Value = true
  })
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("OilCon003_deliv")
  }, DudeKilled, {self})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("OilCon003_deliv"),
    "<",
    60,
    false,
    false
  }, TakeSeat, {self})
  Ai.Role({
    AIGuid = Pg.GetGuidByName("OilCon003_deliv"),
    Role = "Idle",
    Priority = "hiPri"
  })
  local uDangerFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(uDangerFilter, "VZ")
  self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName("OilCon003_deliv"),
    uDangerFilter,
    "<",
    100,
    false,
    false
  }, VZWarning, {self})
end

function TakeSeat(self)
  local tGoalParams = {
    AIGuid = Pg.GetGuidByName("OilCon003_deliv"),
    Goal = "Enter",
    Target = Pg.GetGuidByName("_global_bencha 0x000a0b11"),
    Priority = "hiPri"
  }
  self:_CreateEvent(Event.TimerRelative, {1}, Ai.Goal, {tGoalParams})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("OilCon003_deliv"),
    "<",
    5,
    false,
    false
  }, ExitSeat, {self})
end

function ExitSeat(self)
  Ai.Goal({
    AIGuid = Pg.GetGuidByName("OilCon003_deliv"),
    Goal = "Exit",
    Priority = "hiPri"
  })
end

function VZWarning(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Oil03-04"
  })
end

function DudeKilled(self)
  self:_SetCancelMessage("[OilCon003.Terms.Cancel01]")
  local tVo = {
    "Fiona-In-Mission-MinorContract-Oil03-22",
    "Fiona-In-Mission-MinorContract-Oil03-27",
    "Fiona-In-Mission-MinorContract-Oil03-28"
  }
  local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
  local tSequence = {
    sSelectedVo,
    {
      self.Cancel,
      {self}
    }
  }
  MrxVoSequence.Start(tSequence)
end

function TimeWarningFirst(self)
  local tVo = {
    "OilExec-In-Mission-MinorContract-Oil03-33",
    "OilExec-In-Mission-MinorContract-Oil03-34",
    "OilExec-In-Mission-MinorContract-Oil03-35"
  }
  local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
  MrxVoSequence.Start(sSelectedVo)
  nBonusMult = 2
  nTotalReward = nBonusMult * nBaseReward
  self:_SetPlayer1Bonus(nTotalReward)
  self:_SetPlayer2Bonus(nTotalReward)
  sHudText = "[OilCon003.Objectives.TipPrompt]" .. MrxUtil.FormatMoney(nTotalReward)
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 2,
    sText = sHudText
  })
end

function TimeWarningSecond(self)
  local tVo = {
    "OilExec-In-Mission-MinorContract-Oil03-36",
    "OilExec-In-Mission-MinorContract-Oil03-37",
    "OilExec-In-Mission-MinorContract-Oil03-38"
  }
  local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
  MrxVoSequence.Start(sSelectedVo)
  nBonusMult = 1
  nTotalReward = nBonusMult * nBaseReward
  self:_SetPlayer1Bonus(nTotalReward)
  self:_SetPlayer2Bonus(nTotalReward)
  sHudText = "[OilCon003.Objectives.TipPrompt]" .. MrxUtil.FormatMoney(nTotalReward)
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 2,
    sText = sHudText
  })
end

function DeliverDude(self, uTalked)
  nCompletions = self:GetNumCompletions()
  if nCompletions == 1 then
    nTimeLimit = 240
    nDecreaseRewardTimeFirst = 90
    nDecreaseRewardTimeSecond = 150
    nPurLevel = 2
    nBaseReward = 15000
    uDest = "OilCon3_Drop_Med"
  elseif nCompletions >= 2 then
    nTimeLimit = 200
    nDecreaseRewardTimeFirst = 120
    nDecreaseRewardTimeSecond = 150
    nPurLevel = 3
    nBaseReward = 20000
    uDest = "OilCon3_Drop_Hard"
  else
    nPurLevel = 1
    nTimeLimit = 240
    uDest = "OilCon3_Drop_Easy"
    nDecreaseRewardTimeFirst = 120
    nDecreaseRewardTimeSecond = 180
    nBaseReward = 10000
  end
  nTotalReward = nBonusMult * nBaseReward
  local tVo = {
    "OilExec-In-Mission-MinorContract-Oil03-29",
    "OilExec-In-Mission-MinorContract-Oil03-32"
  }
  local tExecSequence = {
    MrxUtil.GetRandomTableElement(tVo)
  }
  sHudText = "[OilCon003.Objectives.TipPrompt]" .. MrxUtil.FormatMoney(nTotalReward)
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 2,
    sText = sHudText
  })
  SpeedVO(self)
  DamageVehicleVO(self)
  DamageVO(self)
  self:CreateChild({
    sName = "DeliverVip",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = "OilCon003_deliv",
    vDestLoc = uDest,
    sDspShortDesc = "[OilCon003.Objectives.002]",
    fDist = 15,
    uStartAttachedToPlayer = uTalked,
    bStop = true,
    bXZOnly = false,
    nTimeLimit = nTimeLimit,
    fOnComplete = function()
      Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
      local sSelectedVo
      if nCompletions == 1 then
        sSelectedVo = "Fiona-In-Mission-MinorContract-Oil03-07"
      elseif 2 <= nCompletions then
        sSelectedVo = "Fiona-In-Mission-MinorContract-Oil03-13"
      else
        sSelectedVo = "Fiona-In-Mission-MinorContract-Oil03-08"
      end
      local tSequence = {
        sSelectedVo,
        {
          self.Complete,
          {self}
        }
      }
      MrxVoSequence.Start(tSequence)
    end,
    fOnCancel = function()
      Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
      local tVo = {
        "OilExec-In-Mission-MinorContract-Oil03-39",
        "OilExec-In-Mission-MinorContract-Oil03-40"
      }
      local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
      self:_SetCancelMessage("[OilCon003.Terms.Cancel02]")
      local tSequence = {
        sSelectedVo,
        {
          self.Cancel,
          {self}
        }
      }
      MrxVoSequence.Start(tSequence)
    end,
    vVoSeqOnAdd = tExecSequence
  })
  self:_CreateEvent(Event.TimerRelative, {nDecreaseRewardTimeFirst}, TimeWarningFirst, {self})
  self:_CreateEvent(Event.TimerRelative, {nDecreaseRewardTimeSecond}, TimeWarningSecond, {self})
  self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName("OilCon003_deliv"),
    Pg.GetGuidByName("OilCon3_start"),
    ">",
    150,
    false,
    false
  }, StartPursuit, {self, nPurLevel})
  self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName("OilCon003_deliv"),
    Pg.GetGuidByName(uDest),
    "<",
    150,
    false,
    false
  }, ClearPursuit, {self})
  Sound.LockActionLevelMusic(true)
  Sound.SetActionLevelsMusic(3, 0, 0, 0)
end

function Complete(self)
  if nBaseReward and nBonusMult then
    self:_SetPlayer1Bonus(nBaseReward * nBonusMult)
    self:_SetPlayer2Bonus(nBaseReward * nBonusMult)
  end
  MrxTaskContract.Complete(self)
end

function ClearPursuit(self)
  Sound.LockActionLevelMusic(false)
  MrxFactionManager.ClearPursuitLock()
  Debug.Printf("Pursuit OFF *******************")
end

function StartPursuit(self, nPurLevel)
  if nPurLevel == 1 then
    local tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 3}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 3}
        }
      },
      {
        "Offroad",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 2}
        }
      }
    }
    MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("VZ"), -1, tPursuitTable)
  elseif nPurLevel == 2 then
    local tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          },
          {
            "Car",
            "M113 (VZ) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 6}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          },
          {
            "Car",
            "M113 (VZ) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 6}
        }
      },
      {
        "Offroad",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          },
          {
            "Car",
            "M113 (VZ) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 6}
        }
      }
    }
    MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("VZ"), -1, tPursuitTable)
  elseif nPurLevel == 3 then
    local tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          },
          {
            "Tank",
            "Scorpion90 (Full)",
            1
          }
        },
        {
          {"Car", 4},
          {"Tank", 2}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          },
          {
            "Tank",
            "Scorpion90 (Full)",
            1
          }
        },
        {
          {"Car", 4},
          {"Tank", 2}
        }
      },
      {
        "Offroad",
        {
          {
            "Car",
            "M151 .50Cal (VZ) (DriverGunner)",
            1
          },
          {
            "Tank",
            "Scorpion90 (Full)",
            1
          }
        },
        {
          {"Car", 4},
          {"Tank", 2}
        }
      }
    }
    MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("VZ"), -1, tPursuitTable)
  end
end

function SpeedVO(self)
  uDude = Vehicle.GetFromRider(Pg.GetGuidByName("OilCon003_deliv"))
  if uDude then
    nDudeSpeed = Object.GetVelocity(uDude)
    if nDudeSpeed > 30 then
      Debug.Printf("Speeder!11   Going fast!")
      MrxVoSequence.Start({
        "OilExec-In-Mission-MinorContract-Oil03-15",
        {
          mattias = "mattias-In-Mission-MinorContract-Oil03-09",
          jennifer = "jennifer-In-Mission-MinorContract-Oil03-10",
          chris = "chris-In-Mission-MinorContract-Oil03-11"
        }
      })
      self:_CreateEvent(Event.TimerRelative, {30}, SpeedVO, {self})
    else
      self:_CreateEvent(Event.TimerRelative, {2}, SpeedVO, {self})
    end
  else
    self:_CreateEvent(Event.TimerRelative, {5}, SpeedVO, {self})
  end
end

function DamageVehicleVO(self)
  local uCurrentVeh = Vehicle.GetFromRider(Pg.GetGuidByName("OilCon003_deliv"))
  if uCurrentVeh then
    local nOldVehHealth = nVehHealth
    nVehHealth = Object.GetHealth(uCurrentVeh)
    local nTotalDamage = nOldVehHealth - nVehHealth
    if 7 < nTotalDamage then
      MrxVoSequence.Start({
        "OilExec-In-Mission-MinorContract-Oil03-17"
      })
      self:_CreateEvent(Event.TimerRelative, {30}, ResetDamageVehVO, {self})
    else
      self:_CreateEvent(Event.TimerRelative, {3}, DamageVehicleVO, {self})
    end
  else
    self:_CreateEvent(Event.TimerRelative, {3}, DamageVehicleVO, {self})
  end
end

function ResetDamageVehVO(self)
  local uCurrentVeh = Vehicle.GetFromRider(Pg.GetGuidByName("OilCon003_deliv"))
  if uCurrentVeh then
    nVehHealth = Object.GetHealth(uCurrentVeh)
  end
  DamageVehicleVO(self)
end

function DamageVO(self)
  local nOldDudeHealth = nDudeHealth
  nDudeHealth = Object.GetHealth(Pg.GetGuidByName("OilCon003_deliv"))
  if nOldDudeHealth > nDudeHealth then
    MrxVoSequence.Start({
      "OilExec-In-Mission-MinorContract-Oil03-14"
    })
    self:_CreateEvent(Event.TimerRelative, {40}, DamageVO, {self})
  else
    self:_CreateEvent(Event.TimerRelative, {2}, DamageVO, {self})
  end
end

function Cleanup(self)
  MrxFactionManager.ClearPursuitLock()
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
  MrxTaskContract.Cleanup(self)
end
