inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.C4]"
end

function SetupActivationCriteria(self)
  local uPlayer = Player.GetLocalCharacter()
  self:_CreateEvent(Event.WeaponEvent, {
    uPlayer,
    "Equip",
    "c4"
  }, self.ActivateTutorial2, {self})
end

function ActivateTutorial2(self)
  local uPlayer = Player.GetLocalCharacter()
  local tWeapons = Human.Inventory.GetAllWeapons(uPlayer)
  for i, weapon in pairs(tWeapons) do
    local iReserveAmmo = Weapon.GetReserveAmmo(weapon)
    if iReserveAmmo and Object.HasLabel(weapon, "c4") and 0 < iReserveAmmo then
      self:ActivateTutorial(true)
      return
    end
  end
  self:SetupActivationCriteria()
end

function SetupCancellationCriteria(self)
  local uPlayer = Player.GetLocalCharacter()
  self:_CreateEvent(Event.WeaponEvent, {
    uPlayer,
    "Stow",
    "weapon.c4"
  }, self.EndTutorial, {self, false})
end

function SetupCompletionCriteria(self)
  local uPlayer = Player.GetLocalCharacter()
  self:_CreateEvent(Event.HumanStateTransition, {
    uPlayer,
    "*",
    "Upright.TriggerDetonator"
  }, self.EndTutorial, {self, true})
  MrxTutorial.SetupCompletionCriteria(self)
end
