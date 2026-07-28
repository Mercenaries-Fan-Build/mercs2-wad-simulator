inherit("MrxTaskJobDestroySet")
_tTargetNearbyVo = {
  {
    vSequence = "Fiona-None-Freeplay-None-08",
    tRange = {
      "[",
      1,
      10,
      "]"
    }
  },
  {
    vSequence = "Fiona-None-Freeplay-None-12",
    tRange = {
      "[",
      11,
      23,
      "]"
    }
  },
  {
    vSequence = "Fiona-None-Freeplay-None-07",
    tRange = {24}
  }
}
_tTargetCompleteVo = {
  {
    vSequence = "Fiona-In-Mission-Job-All05-06",
    tRange = {1}
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-08",
    tRange = {
      "[",
      2,
      4,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-07",
    tRange = {
      "[",
      5,
      8,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-10",
    tRange = {
      "[",
      9,
      11,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-All06-06",
    tRange = {
      "[",
      12,
      15,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-06",
    tRange = {
      "[",
      16,
      17,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-All10-06",
    tRange = {
      "[",
      18,
      19,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-All10-10",
    tRange = {
      "[",
      20,
      21,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Gur07-08",
    tRange = {
      "[",
      22,
      23,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-All05-07",
    tRange = {24}
  }
}

function LoadAssets(self, tSaveData)
  self:_AddTarget({
    sTarget = "AllJob005_Target01",
    sPristineLayer = "Vz_State_AllJob005_01",
    sDefenseLayer = "Vz_State_AllJob005_01_Defenses",
    sDestroyedLayer = "Vz_State_AllJob005_01_Destroyed",
    sStagingLayer = "Vz_State_AllJob005_01_Staging"
  })
  self:_AddTarget({
    sTarget = "AllJob005_Target02",
    sPristineLayer = "Vz_State_AllJob005_02",
    sDefenseLayer = "Vz_State_AllJob005_02_Defenses",
    sDestroyedLayer = "Vz_State_AllJob005_02_Destroyed",
    sStagingLayer = "Vz_State_AllJob005_02_Staging"
  })
  self:_AddTarget({
    sTarget = "AllJob005_Target03",
    sPristineLayer = "Vz_State_AllJob005_03",
    sDefenseLayer = "Vz_State_AllJob005_03_Defenses",
    sDestroyedLayer = "Vz_State_AllJob005_03_Destroyed",
    sStagingLayer = "Vz_State_AllJob005_03_Staging"
  })
  self:_AddTarget({
    sTarget = "AllJob005_Target04",
    sPristineLayer = "Vz_State_AllJob005_04",
    sDefenseLayer = "Vz_State_AllJob005_04_Defenses",
    sDestroyedLayer = "Vz_State_AllJob005_04_Destroyed",
    sStagingLayer = "Vz_State_AllJob005_04_Staging"
  })
  self:_AddTarget({
    sTarget = "AllJob005_Target05",
    sPristineLayer = "Vz_State_AllJob005_05",
    sDefenseLayer = "Vz_State_AllJob005_05_Defenses",
    sDestroyedLayer = "Vz_State_AllJob005_05_Destroyed",
    sStagingLayer = "Vz_State_AllJob005_05_Staging"
  })
  self:_AddTarget({
    sTarget = "AllJob009_Target01",
    sPristineLayer = "Vz_State_AllJob009_01",
    sDefenseLayer = "Vz_State_AllJob009_01_Defenses",
    sDestroyedLayer = "Vz_State_AllJob009_01_Destroyed",
    sStagingLayer = "Vz_State_AllJob009_01_Staging"
  })
  self:_AddTarget({
    sTarget = "AllJob009_Target02",
    sPristineLayer = "Vz_State_AllJob009_02",
    sDefenseLayer = "Vz_State_AllJob009_02_Defenses",
    sDestroyedLayer = "Vz_State_AllJob009_02_Destroyed",
    sStagingLayer = "Vz_State_AllJob009_02_Staging"
  })
  self:_AddTarget({
    sTarget = "AllJob009_Target03",
    sPristineLayer = "Vz_State_AllJob009_03",
    sDefenseLayer = "Vz_State_AllJob009_03_Defenses",
    sDestroyedLayer = "Vz_State_AllJob009_03_Destroyed",
    sStagingLayer = "Vz_State_AllJob009_03_Staging"
  })
  self:_AddTarget({
    sTarget = "AllJob009_Target04",
    sPristineLayer = "Vz_State_AllJob009_04",
    sDefenseLayer = "Vz_State_AllJob009_04_Defenses",
    sDestroyedLayer = "Vz_State_AllJob009_04_Destroyed",
    sStagingLayer = "Vz_State_AllJob009_04_Staging"
  })
  self:_AddTarget({
    sTarget = "AllJob009_Target05",
    sPristineLayer = "Vz_State_AllJob009_05",
    sDefenseLayer = "Vz_State_AllJob009_05_Defenses",
    sDestroyedLayer = "Vz_State_AllJob009_05_Destroyed",
    sStagingLayer = "Vz_State_AllJob009_05_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_A_Objective1",
    sPristineLayer = "Vz_State_ChiJob006_A_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_A_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_A_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_A_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_B_Objective1",
    sPristineLayer = "Vz_State_ChiJob006_B_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_B_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_B_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_B_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_C_Objective0",
    sPristineLayer = "Vz_State_ChiJob006_C_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_C_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_C_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_C_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_C_Objective2",
    sPristineLayer = "Vz_State_ChiJob006_C_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_C_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_C_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_C_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_C_Objective3",
    sPristineLayer = "Vz_State_ChiJob006_C_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_C_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_C_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_C_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_C_Objective4",
    sPristineLayer = "Vz_State_ChiJob006_C_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_C_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_C_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_C_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_C_Objective5",
    sPristineLayer = "Vz_State_ChiJob006_C_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_C_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_C_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_C_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_C_Objective6",
    sPristineLayer = "Vz_State_ChiJob006_C_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_C_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_C_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_C_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_C_Objective8",
    sPristineLayer = "Vz_State_ChiJob006_C_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_C_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_C_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_C_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_C_Objective9",
    sPristineLayer = "Vz_State_ChiJob006_C_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_C_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_C_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_C_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_D_Objective1",
    sPristineLayer = "Vz_State_ChiJob006_D_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_D_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_D_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_D_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_D_Objective5",
    sPristineLayer = "Vz_State_ChiJob006_D_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_D_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_D_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_D_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_D_Objective6",
    sPristineLayer = "Vz_State_ChiJob006_D_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_D_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_D_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_D_Staging"
  })
  self:_AddTarget({
    sTarget = "ChiJob006_E_Objective1",
    sPristineLayer = "Vz_State_ChiJob006_E_Pristine",
    sDefenseLayer = "Vz_State_ChiJob006_E_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob006_E_Destroyed",
    sStagingLayer = "Vz_State_ChiJob006_E_Staging"
  })
  MrxTaskJobDestroySet.LoadAssets(self, tSaveData)
end

function Activated(self)
  MrxTaskJobDestroySet.Activated(self)
  self:_SetTargetNearbyVo(_tTargetNearbyVo)
  self:_SetTargetCompleteVo(_tTargetCompleteVo)
  self:_Go()
end

function Cleanup(self)
  MrxTaskJobDestroySet.Cleanup(self)
end
